//! MongoDB (official `mongodb` driver).
//!
//! Documents cross the IPC boundary as JSON close to MongoDB's relaxed
//! Extended JSON: ObjectIds, dates and decimals as `{"$oid": ...}`,
//! `{"$date": ...}`, `{"$numberDecimal": ...}`, plain numbers for ints and
//! doubles, and `{"$numberLong": ...}` only for 64-bit integers a JavaScript
//! number can't hold exactly. Saving reads the stored document first and keeps
//! each field's BSON type (an Int64 stays Int64 even when it looks small).
//!
//! The query console takes mongosh-style statements:
//!   db.users.find({plan: "pro"}).sort({age: -1}).limit(10)
//!   db.orders.aggregate([{$group: {_id: "$status", n: {$sum: 1}}}])
//!   db.users.countDocuments({}), db.users.distinct("plan")
//!   db.users.insertOne({...}), updateOne/updateMany, deleteOne/deleteMany
//!   use shop, show dbs, show collections

use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::ClientOptions;
use mongodb::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MongoConfig {
    pub name: String,
    /// `mongodb://...` or `mongodb+srv://...`
    pub uri: String,
    /// Database for `db.` in the console; the URI's, else the first one found.
    #[serde(default)]
    pub database: Option<String>,
}

pub struct MongoConn {
    pub client: Client,
    /// The console's current database (`use <db>` changes it).
    pub current_db: Mutex<String>,
}

pub type MongoHandle = Arc<MongoConn>;

const SYSTEM_DBS: &[&str] = &["admin", "config", "local"];

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub async fn connect(cfg: &MongoConfig) -> Result<MongoHandle, String> {
    let uri = cfg.uri.trim();
    if !(uri.starts_with("mongodb://") || uri.starts_with("mongodb+srv://")) {
        return Err("The connection string starts with mongodb:// or mongodb+srv://".into());
    }
    let mut opts = tokio::time::timeout(Duration::from_secs(20), ClientOptions::parse(uri))
        .await
        .map_err(|_| "Resolving the connection string timed out".to_string())?
        .map_err(|e| format!("Invalid connection string: {e}"))?;
    opts.server_selection_timeout = Some(Duration::from_secs(10));
    opts.connect_timeout = Some(Duration::from_secs(10));
    opts.app_name = Some("Stroke".into());
    let uri_db = opts.default_database.clone();
    let client = Client::with_options(opts).map_err(err)?;
    client
        .database("admin")
        .run_command(doc! { "ping": 1 })
        .await
        .map_err(|e| format!("Could not reach MongoDB: {e}"))?;
    let db = match cfg.database.as_deref().map(str::trim).filter(|d| !d.is_empty()).map(str::to_string).or(uri_db) {
        Some(d) => d,
        None => client
            .list_database_names()
            .await
            .ok()
            .and_then(|names| names.into_iter().find(|n| !SYSTEM_DBS.contains(&n.as_str())))
            .unwrap_or_else(|| "test".into()),
    };
    Ok(Arc::new(MongoConn { client, current_db: Mutex::new(db) }))
}

// ── JSON <-> BSON ────────────────────────────────────────────────────────────

const SAFE_INT: i64 = 9_007_199_254_740_991;

/// BSON as the app's JSON (see the module docs).
pub fn to_json(b: &Bson) -> Value {
    match b {
        Bson::Document(d) => doc_to_json(d),
        Bson::Array(a) => Value::Array(a.iter().map(to_json).collect()),
        Bson::String(s) => Value::String(s.clone()),
        Bson::Boolean(v) => Value::Bool(*v),
        Bson::Null => Value::Null,
        Bson::Int32(i) => json!(i),
        Bson::Int64(i) if i.abs() <= SAFE_INT => json!(i),
        Bson::Int64(i) => json!({ "$numberLong": i.to_string() }),
        Bson::Double(f) if f.is_finite() => json!(f),
        other => other.clone().into_relaxed_extjson(),
    }
}

pub fn doc_to_json(d: &Document) -> Value {
    Value::Object(d.iter().map(|(k, v)| (k.clone(), to_json(v))).collect())
}

/// An Extended JSON wrapper such as `{"$oid": ...}`, `{"$date": ...}`.
fn is_extjson(o: &serde_json::Map<String, Value>) -> bool {
    !o.is_empty() && o.keys().all(|k| k.starts_with('$'))
        && o.keys().any(|k| {
            matches!(
                k.as_str(),
                "$oid" | "$date" | "$numberLong" | "$numberInt" | "$numberDouble" | "$numberDecimal" | "$binary" | "$uuid"
                    | "$regularExpression" | "$timestamp" | "$symbol" | "$code" | "$minKey" | "$maxKey" | "$undefined"
            )
        })
}

/// JSON back to BSON. `hint` is the stored value at the same place, whose
/// numeric type wins where the new value still fits it.
pub fn to_bson(v: &Value, hint: Option<&Bson>) -> Result<Bson, String> {
    Ok(match v {
        Value::Null => Bson::Null,
        Value::Bool(b) => Bson::Boolean(*b),
        Value::String(s) => Bson::String(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                match hint {
                    Some(Bson::Int64(_)) => Bson::Int64(i),
                    Some(Bson::Double(_)) => Bson::Double(i as f64),
                    _ => i32::try_from(i).map(Bson::Int32).unwrap_or(Bson::Int64(i)),
                }
            } else if let Some(f) = n.as_f64() {
                Bson::Double(f)
            } else {
                return Err(format!("{n} is out of range"));
            }
        }
        Value::Array(a) => {
            let hs = match hint { Some(Bson::Array(h)) => Some(h), _ => None };
            Bson::Array(a.iter().enumerate().map(|(i, x)| to_bson(x, hs.and_then(|h| h.get(i)))).collect::<Result<_, _>>()?)
        }
        Value::Object(o) if is_extjson(o) => Bson::try_from(v.clone()).map_err(|e| format!("Invalid {}: {e}", o.keys().next().unwrap()))?,
        Value::Object(o) => {
            let hd = match hint { Some(Bson::Document(h)) => Some(h), _ => None };
            let mut d = Document::new();
            for (k, x) in o {
                d.insert(k.clone(), to_bson(x, hd.and_then(|h| h.get(k)))?);
            }
            Bson::Document(d)
        }
    })
}

pub fn to_doc(v: &Value, hint: Option<&Document>) -> Result<Document, String> {
    match to_bson(v, hint.map(|h| Bson::Document(h.clone())).as_ref())? {
        Bson::Document(d) => Ok(d),
        _ => Err("A document is a JSON object".into()),
    }
}

// ── mongosh-style literals ──────────────────────────────────────────────────

/// Rewrite shell helpers into Extended JSON so json5 can read them:
/// `ObjectId("x")`, `ISODate("x")`, `new Date("x")`, `NumberLong(x)`,
/// `NumberInt(x)`, `NumberDecimal("x")`, `UUID("x")`.
pub fn shell_to_json5(src: &str) -> String {
    const HELPERS: &[(&str, &str)] = &[
        ("new Date(", "$date"),
        ("ObjectId(", "$oid"),
        ("ISODate(", "$date"),
        ("NumberLong(", "$numberLong"),
        ("NumberDecimal(", "$numberDecimal"),
        ("NumberInt(", "$int"),
        ("UUID(", "$uuid"),
    ];
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let bytes = src.as_bytes();
    let mut in_str: Option<u8> = None;
    while i < src.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            out.push(c as char);
            if c == b'\\' && i + 1 < src.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            in_str = Some(c);
            out.push(c as char);
            i += 1;
            continue;
        }
        let hit = HELPERS.iter().find(|(name, _)| src[i..].starts_with(name) && (i == 0 || !(bytes[i - 1] as char).is_alphanumeric()));
        if let Some((name, key)) = hit {
            let start = i + name.len();
            if let Some(end) = src[start..].find(')') {
                let arg = src[start..start + end].trim().trim_matches(|c| c == '"' || c == '\'');
                let arg = if *key == "$date" && arg.len() == 10 { format!("{arg}T00:00:00Z") } else { arg.to_string() };
                if *key == "$int" {
                    out.push_str(&arg);
                } else {
                    out.push_str(&format!("{{\"{key}\": \"{}\"}}", arg.replace('"', "\\\"")));
                }
                i = start + end + 1;
                continue;
            }
        }
        out.push(c as char);
        i += 1;
    }
    out
}

/// Parse shell-style arguments (`{plan: "pro"}, {age: 1}`) into JSON values.
pub fn parse_args(src: &str) -> Result<Vec<Value>, String> {
    let s = src.trim();
    if s.is_empty() {
        return Ok(vec![]);
    }
    json5::from_str::<Vec<Value>>(&format!("[{}]", shell_to_json5(s))).map_err(|e| format!("Couldn't read the arguments: {e}"))
}

/// One filter (or sort) object from the browser's filter box.
pub fn parse_object(src: &str, what: &str) -> Result<Document, String> {
    let s = src.trim();
    if s.is_empty() {
        return Ok(Document::new());
    }
    let v: Value = json5::from_str(&shell_to_json5(s)).map_err(|e| format!("The {what} isn't valid: {e}"))?;
    if !v.is_object() {
        return Err(format!("The {what} is an object, e.g. {{plan: \"pro\"}}"));
    }
    to_doc(&v, None)
}

/// The browser's filter box: `parse_object`, then a 24-hex string also
/// matches the ObjectId it spells (see `coerce_oid_strings`).
pub fn parse_filter(src: &str) -> Result<Document, String> {
    Ok(coerce_oid_strings(parse_object(src, "filter")?))
}

fn hex_oid(b: &Bson) -> Option<mongodb::bson::oid::ObjectId> {
    match b {
        Bson::String(s) if s.len() == 24 && s.bytes().all(|c| c.is_ascii_hexdigit()) => mongodb::bson::oid::ObjectId::parse_str(s).ok(),
        _ => None,
    }
}

/// Typing `{userId: "6af4…"}` finds nothing when userId holds ObjectIds. Each
/// equality on a 24-hex string becomes `{$in: [string, ObjectId]}`, `$ne`
/// becomes `$nin` of both, and `$in` / `$nin` lists gain the ObjectId beside
/// the string. `$expr`, `$regex`, `$where` and the like are left alone.
pub fn coerce_oid_strings(filter: Document) -> Document {
    let mut out = Document::new();
    for (k, v) in filter {
        let v = match (k.as_str(), v) {
            ("$and" | "$or" | "$nor", Bson::Array(a)) => {
                Bson::Array(a.into_iter().map(|x| match x { Bson::Document(d) => Bson::Document(coerce_oid_strings(d)), o => o }).collect())
            }
            (k, v) if k.starts_with('$') => v,
            (_, v) => coerce_field(v),
        };
        out.insert(k, v);
    }
    out
}

/// A field's condition: a bare value, or an operator document.
fn coerce_field(v: Bson) -> Bson {
    if let Some(oid) = hex_oid(&v) {
        return Bson::Document(doc! { "$in": [v, oid] });
    }
    let Bson::Document(d) = v else { return v };
    if d.is_empty() || !d.keys().all(|k| k.starts_with('$')) {
        return Bson::Document(d);
    }
    let both = |x: Bson| -> Vec<Bson> {
        match hex_oid(&x) {
            Some(o) => vec![x, Bson::ObjectId(o)],
            None => vec![x],
        }
    };
    let mut out = Document::new();
    for (op, x) in d {
        match (op.as_str(), x) {
            ("$eq", x) if hex_oid(&x).is_some() && !out.contains_key("$in") => {
                out.insert("$in", both(x));
            }
            ("$ne", x) if hex_oid(&x).is_some() && !out.contains_key("$nin") => {
                out.insert("$nin", both(x));
            }
            ("$in" | "$nin", Bson::Array(a)) => {
                let mut list = match out.remove(&op) { Some(Bson::Array(prev)) => prev, _ => vec![] };
                list.extend(a.into_iter().flat_map(both));
                out.insert(op, list);
            }
            ("$elemMatch", Bson::Document(inner)) => {
                out.insert(op, coerce_oid_strings(inner));
            }
            ("$not", x) => {
                out.insert(op, coerce_field(x));
            }
            (_, x) => {
                out.insert(op, x);
            }
        }
    }
    Bson::Document(out)
}

// ── Browser ─────────────────────────────────────────────────────────────────

fn split(full: &str) -> Result<(&str, &str), String> {
    full.split_once('.').ok_or_else(|| format!("\"{full}\" is not database.collection"))
}

/// User databases (not admin/config/local), sorted.
pub async fn list_databases(m: &MongoConn) -> Result<Vec<String>, String> {
    let mut dbs: Vec<String> = m.client.list_database_names().await.map_err(err)?.into_iter().filter(|d| !SYSTEM_DBS.contains(&d.as_str())).collect();
    dbs.sort();
    Ok(dbs)
}

/// One database's collections (not `system.*`), sorted.
pub async fn collections_in(m: &MongoConn, db: &str) -> Result<Vec<String>, String> {
    let mut names = m.client.database(db).list_collection_names().await.map_err(err)?;
    names.retain(|n| !n.starts_with("system."));
    names.sort();
    Ok(names)
}

/// Point the console's `db` at a database (what `use <db>` does).
pub fn use_db(m: &MongoConn, db: &str) {
    if let Ok(mut g) = m.current_db.lock() {
        *g = db.to_string();
    }
}

/// Collections of every user database, as `db.collection`.
pub async fn list_collections(m: &MongoConn) -> Result<Vec<String>, String> {
    let dbs = m.client.list_database_names().await.map_err(err)?;
    let mut out = vec![];
    for db in dbs.into_iter().filter(|d| !SYSTEM_DBS.contains(&d.as_str())) {
        let mut names = m.client.database(&db).list_collection_names().await.map_err(err)?;
        names.retain(|n| !n.starts_with("system."));
        names.sort();
        out.extend(names.into_iter().map(|n| format!("{db}.{n}")));
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct CollInfo {
    pub count: u64,
    pub indexes: Vec<String>,
    pub size: Option<f64>,
    pub capped: bool,
}

pub async fn describe(m: &MongoConn, full: &str) -> Result<CollInfo, String> {
    let (db, coll) = split(full)?;
    let c = m.client.database(db).collection::<Document>(coll);
    let count = c.estimated_document_count().await.map_err(err)?;
    let indexes = c.list_index_names().await.unwrap_or_default();
    let stats = m.client.database(db).run_command(doc! { "collStats": coll }).await.ok();
    let num = |k: &str| stats.as_ref().and_then(|s| s.get(k)).and_then(|v| match v {
        Bson::Int32(i) => Some(*i as f64),
        Bson::Int64(i) => Some(*i as f64),
        Bson::Double(f) => Some(*f),
        _ => None,
    });
    Ok(CollInfo {
        count,
        indexes,
        size: num("size"),
        capped: stats.as_ref().and_then(|s| s.get_bool("capped").ok()).unwrap_or(false),
    })
}

/// One collection's indexes: name, key fields in order (`-1` for descending,
/// the kind for `text` / `2dsphere` / `hashed` keys), and whether it is unique.
pub async fn index_list(m: &MongoConn, db: &str, coll: &str) -> Result<Vec<super::schema::IndexInfo>, String> {
    let c = m.client.database(db).collection::<Document>(coll);
    let models: Vec<mongodb::IndexModel> = c.list_indexes().await.map_err(err)?.try_collect().await.map_err(err)?;
    Ok(models
        .iter()
        .map(|ix| {
            let name = ix.options.as_ref().and_then(|o| o.name.clone()).unwrap_or_default();
            let unique = ix.options.as_ref().and_then(|o| o.unique).unwrap_or(false);
            index_from_keys(coll, name, &ix.keys, unique)
        })
        .collect())
}

pub fn index_from_keys(coll: &str, name: String, keys: &Document, unique: bool) -> super::schema::IndexInfo {
    let mut kind = String::new();
    let cols = keys
        .iter()
        .map(|(field, dir)| match dir {
            Bson::String(k) => {
                kind = k.clone();
                format!("{field} {k}")
            }
            Bson::Int32(-1) | Bson::Int64(-1) => format!("{field} desc"),
            Bson::Double(d) if *d < 0.0 => format!("{field} desc"),
            _ => field.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let primary = name == "_id_";
    super::schema::IndexInfo {
        name,
        table_name: coll.to_string(),
        columns: cols,
        index_type: if kind.is_empty() { "btree".into() } else { kind },
        is_unique: unique || primary,
        is_primary: primary,
        condition: None,
        comment: None,
    }
}

#[derive(Serialize)]
pub struct DocPage {
    pub docs: Vec<Value>,
    pub next: Option<u64>,
}

/// One page, with an optional projection (Compass's Project option).
pub async fn find_page_with(m: &MongoConn, full: &str, filter: &str, sort: &str, projection: &str, skip: u64, limit: i64) -> Result<DocPage, String> {
    let (db, coll) = split(full)?;
    let filter = parse_filter(filter)?;
    let sort = parse_object(sort, "sort")?;
    let projection = parse_object(projection, "projection")?;
    let limit = limit.clamp(1, 1000);
    let c = m.client.database(db).collection::<Document>(coll);
    let mut q = c.find(filter).skip(skip).limit(limit + 1);
    if !sort.is_empty() {
        q = q.sort(sort);
    }
    if !projection.is_empty() {
        q = q.projection(projection);
    }
    let mut docs: Vec<Document> = q.await.map_err(err)?.try_collect().await.map_err(err)?;
    let more = docs.len() as i64 > limit;
    docs.truncate(limit as usize);
    Ok(DocPage {
        next: more.then_some(skip + limit as u64),
        docs: docs.iter().map(doc_to_json).collect(),
    })
}

/// Save a document. `prev_id` is the `_id` it was loaded with (JSON form);
/// `None` for a new document. Returns the stored document.
pub async fn save(m: &MongoConn, full: &str, doc_json: &Value, prev_id: Option<&Value>) -> Result<Value, String> {
    let (db, coll) = split(full)?;
    let c = m.client.database(db).collection::<Document>(coll);
    let prev_id = prev_id.map(|p| to_bson(p, None)).transpose()?;
    // Types come from the stored version, when there is one.
    let stored = match &prev_id {
        Some(id) => c.find_one(doc! { "_id": id.clone() }).await.map_err(err)?,
        None => None,
    };
    if prev_id.is_some() && stored.is_none() {
        return Err("This document no longer exists; reload and try again".into());
    }
    let mut new_doc = to_doc(doc_json, stored.as_ref())?;
    // An empty `_id` means "generate one".
    if matches!(new_doc.get("_id"), Some(Bson::String(s)) if s.trim().is_empty()) {
        new_doc.remove("_id");
    }
    let new_id = new_doc.get("_id").cloned();
    let id = match (&prev_id, &new_id) {
        (Some(old), Some(new)) if old == new => {
            c.replace_one(doc! { "_id": old.clone() }, new_doc).await.map_err(err)?;
            old.clone()
        }
        (Some(old), _) => {
            // A changed _id: insert under the new one, then drop the old.
            let r = c.insert_one(new_doc).await.map_err(|e| dup_key(e.to_string()))?;
            c.delete_one(doc! { "_id": old.clone() }).await.map_err(err)?;
            r.inserted_id
        }
        (None, _) => c.insert_one(new_doc).await.map_err(|e| dup_key(e.to_string()))?.inserted_id,
    };
    let back = c.find_one(doc! { "_id": id }).await.map_err(err)?.ok_or("The saved document could not be read back")?;
    Ok(doc_to_json(&back))
}

fn dup_key(e: String) -> String {
    if e.contains("E11000") {
        "A document with this _id already exists".into()
    } else {
        e
    }
}

/// Documents matching a filter (exact; the unfiltered count is the cheap
/// estimate from collection metadata).
pub async fn count(m: &MongoConn, full: &str, filter: &str) -> Result<u64, String> {
    let (db, coll) = split(full)?;
    let filter = parse_filter(filter)?;
    let c = m.client.database(db).collection::<Document>(coll);
    if filter.is_empty() {
        c.estimated_document_count().await.map_err(err)
    } else {
        c.count_documents(filter).await.map_err(err)
    }
}

pub async fn delete(m: &MongoConn, full: &str, id: &Value) -> Result<(), String> {
    let (db, coll) = split(full)?;
    let r = m.client.database(db).collection::<Document>(coll).delete_one(doc! { "_id": to_bson(id, None)? }).await.map_err(err)?;
    if r.deleted_count == 0 {
        return Err("Nothing was deleted: the document is already gone".into());
    }
    Ok(())
}

// ── Explain ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExplainStage {
    pub stage: String,
    pub depth: u32,
    pub index_name: Option<String>,
    pub key_pattern: Option<Value>,
    pub keys_examined: Option<i64>,
    pub docs_examined: Option<i64>,
    pub n_returned: Option<i64>,
    pub ms: Option<i64>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExplainSummary {
    pub stages: Vec<ExplainStage>,
    pub indexes: Vec<String>,
    pub collscan: bool,
    pub n_returned: Option<i64>,
    pub keys_examined: Option<i64>,
    pub docs_examined: Option<i64>,
    pub ms: Option<i64>,
    pub verdict: String,
    pub raw: Value,
}

fn num_of(d: &Document, k: &str) -> Option<i64> {
    match d.get(k)? {
        Bson::Int32(i) => Some(*i as i64),
        Bson::Int64(i) => Some(*i),
        Bson::Double(f) => Some(*f as i64),
        _ => None,
    }
}

const CHILD_KEYS: &[&str] = &["inputStage", "innerStage", "outerStage", "thenStage", "elseStage", "queryPlan"];

fn flatten_stages(d: &Document, depth: u32, out: &mut Vec<ExplainStage>) {
    // SBE plans wrap the tree as {queryPlan: {...}, slotBasedPlan: ...}.
    if !d.contains_key("stage") {
        if let Ok(q) = d.get_document("queryPlan") {
            flatten_stages(q, depth, out);
        }
        return;
    }
    out.push(ExplainStage {
        stage: d.get_str("stage").unwrap_or("?").to_string(),
        depth,
        index_name: d.get_str("indexName").ok().map(str::to_string),
        key_pattern: d.get_document("keyPattern").ok().map(doc_to_json),
        keys_examined: num_of(d, "keysExamined"),
        docs_examined: num_of(d, "docsExamined"),
        n_returned: num_of(d, "nReturned"),
        ms: num_of(d, "executionTimeMillisEstimate"),
    });
    for k in CHILD_KEYS {
        if let Ok(c) = d.get_document(k) {
            flatten_stages(c, depth + 1, out);
        }
    }
    if let Ok(cs) = d.get_array("inputStages") {
        for c in cs {
            if let Bson::Document(c) = c {
                flatten_stages(c, depth + 1, out);
            }
        }
    }
}

/// A find/aggregate explain (executionStats) as a stage tree and a verdict.
pub fn summarize_explain(raw: &Document) -> ExplainSummary {
    // aggregate: the find part sits in stages[0].$cursor.
    let base = raw
        .get_array("stages")
        .ok()
        .and_then(|s| s.first())
        .and_then(|s| s.as_document())
        .and_then(|s| s.get_document("$cursor").ok())
        .unwrap_or(raw);
    let exec = base.get_document("executionStats").ok();
    let mut stages = vec![];
    // Prefer the executed tree (it has the numbers) when it is the classic one.
    match exec.and_then(|e| e.get_document("executionStages").ok()) {
        Some(es) if es.contains_key("stage") && !es.contains_key("opType") => flatten_stages(es, 0, &mut stages),
        _ => {}
    }
    if stages.is_empty() {
        if let Some(wp) = base.get_document("queryPlanner").ok().and_then(|q| q.get_document("winningPlan").ok()) {
            flatten_stages(wp, 0, &mut stages);
        }
    }
    let mut indexes: Vec<String> = vec![];
    for s in &stages {
        if let Some(n) = &s.index_name {
            if !indexes.contains(n) {
                indexes.push(n.clone());
            }
        }
    }
    let up = |s: &str| s.to_ascii_uppercase();
    if indexes.is_empty() && stages.iter().any(|s| matches!(up(&s.stage).as_str(), "IDHACK" | "EXPRESS_IXSCAN" | "EXPRESS_CLUSTERED_IXSCAN")) {
        indexes.push("_id_".into());
    }
    let collscan = stages.iter().any(|s| up(&s.stage) == "COLLSCAN");
    let n_returned = exec.and_then(|e| num_of(e, "nReturned"));
    let keys_examined = exec.and_then(|e| num_of(e, "totalKeysExamined"));
    let docs_examined = exec.and_then(|e| num_of(e, "totalDocsExamined"));
    let ms = exec.and_then(|e| num_of(e, "executionTimeMillis"));
    let fmt = |n: Option<i64>| n.map(|n| group_digits(n)).unwrap_or_else(|| "?".into());
    let verdict = if collscan {
        format!("Collection scan — examined {} to return {}", fmt(docs_examined), fmt(n_returned))
    } else if !indexes.is_empty() {
        format!("Used index {} — examined {} keys, {} documents to return {}", indexes.join(", "), fmt(keys_examined), fmt(docs_examined), fmt(n_returned))
    } else if stages.iter().any(|s| up(&s.stage) == "EOF") {
        "No collection to scan — nothing to return".into()
    } else {
        format!("Examined {} documents to return {}", fmt(docs_examined), fmt(n_returned))
    };
    ExplainSummary { stages, indexes, collscan, n_returned, keys_examined, docs_examined, ms, verdict, raw: doc_to_json(raw) }
}

fn group_digits(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// Explain the browser's current find (executionStats).
pub async fn explain(m: &MongoConn, full: &str, filter: &str, sort: &str, projection: &str) -> Result<ExplainSummary, String> {
    let (db, coll) = split(full)?;
    let mut find = doc! { "find": coll, "filter": parse_filter(filter)? };
    let sort = parse_object(sort, "sort")?;
    if !sort.is_empty() {
        find.insert("sort", sort);
    }
    let projection = parse_object(projection, "projection")?;
    if !projection.is_empty() {
        find.insert("projection", projection);
    }
    let raw = m.client.database(db).run_command(doc! { "explain": find, "verbosity": "executionStats" }).await.map_err(err)?;
    Ok(summarize_explain(&raw))
}

// ── Schema ───────────────────────────────────────────────────────────────────

pub fn bson_type_name(b: &Bson) -> &'static str {
    match b {
        Bson::Double(_) => "double",
        Bson::String(_) => "string",
        Bson::Array(_) => "array",
        Bson::Document(_) => "object",
        Bson::Boolean(_) => "bool",
        Bson::Null | Bson::Undefined => "null",
        Bson::RegularExpression(_) => "regex",
        Bson::JavaScriptCode(_) | Bson::JavaScriptCodeWithScope(_) => "javascript",
        Bson::Int32(_) => "int",
        Bson::Int64(_) => "long",
        Bson::Timestamp(_) => "timestamp",
        Bson::Binary(_) => "binData",
        Bson::ObjectId(_) => "objectId",
        Bson::DateTime(_) => "date",
        Bson::Decimal128(_) => "decimal",
        Bson::Symbol(_) => "symbol",
        Bson::MaxKey => "maxKey",
        Bson::MinKey => "minKey",
        Bson::DbPointer(_) => "dbPointer",
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FieldStat {
    pub path: String,
    /// Documents holding the field (any type).
    pub count: u64,
    /// `count` over the sampled documents, 0–1.
    pub presence: f64,
    /// (type, values seen), most common first.
    pub types: Vec<(String, u64)>,
    /// (value, count), most frequent first; scalars only.
    pub top: Vec<(Value, u64)>,
    pub distinct: u64,
    /// True when `distinct` stopped counting at the cap.
    pub distinct_capped: bool,
    pub min: Option<Value>,
    pub max: Option<Value>,
}

#[derive(Default)]
struct Acc {
    count: u64,
    types: Vec<(&'static str, u64)>,
    values: std::collections::HashMap<String, (Value, u64)>,
    capped: bool,
    min: Option<(f64, Bson)>,
    max: Option<(f64, Bson)>,
}

const DISTINCT_CAP: usize = 500;

impl Acc {
    fn add(&mut self, v: &Bson) {
        let t = bson_type_name(v);
        match self.types.iter_mut().find(|(n, _)| *n == t) {
            Some(e) => e.1 += 1,
            None => self.types.push((t, 1)),
        }
        let ord = match v {
            Bson::Int32(i) => Some(*i as f64),
            Bson::Int64(i) => Some(*i as f64),
            Bson::Double(f) if f.is_finite() => Some(*f),
            Bson::Decimal128(d) => d.to_string().parse::<f64>().ok(),
            Bson::DateTime(d) => Some(d.timestamp_millis() as f64),
            _ => None,
        };
        if let Some(o) = ord {
            if self.min.as_ref().is_none_or(|(m, _)| o < *m) {
                self.min = Some((o, v.clone()));
            }
            if self.max.as_ref().is_none_or(|(m, _)| o > *m) {
                self.max = Some((o, v.clone()));
            }
        }
        if matches!(v, Bson::Document(_) | Bson::Array(_) | Bson::Binary(_)) {
            return;
        }
        let j = to_json(v);
        let key = j.to_string();
        if let Some(e) = self.values.get_mut(&key) {
            e.1 += 1;
        } else if self.values.len() < DISTINCT_CAP {
            self.values.insert(key, (j, 1));
        } else {
            self.capped = true;
        }
    }
}

/// Per field path (`a.b`, arrays as `tags[]`), across `docs`: presence, type
/// mix, frequent values and numeric/date range.
pub fn infer_schema(docs: &[Document]) -> Vec<FieldStat> {
    let mut order: Vec<String> = vec![];
    let mut accs: std::collections::HashMap<String, Acc> = Default::default();
    fn walk(prefix: &str, d: &Document, order: &mut Vec<String>, accs: &mut std::collections::HashMap<String, Acc>, seen: &mut std::collections::HashSet<String>) {
        for (k, v) in d {
            let path = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
            visit(&path, v, order, accs, seen);
        }
    }
    fn visit(path: &str, v: &Bson, order: &mut Vec<String>, accs: &mut std::collections::HashMap<String, Acc>, seen: &mut std::collections::HashSet<String>) {
        let acc = accs.entry(path.to_string()).or_insert_with(|| {
            order.push(path.to_string());
            Acc::default()
        });
        if seen.insert(path.to_string()) {
            acc.count += 1;
        }
        acc.add(v);
        match v {
            Bson::Document(inner) => walk(path, inner, order, accs, seen),
            Bson::Array(items) => {
                let ep = format!("{path}[]");
                for it in items {
                    visit(&ep, it, order, accs, seen);
                }
            }
            _ => {}
        }
    }
    for d in docs {
        let mut seen = std::collections::HashSet::new();
        walk("", d, &mut order, &mut accs, &mut seen);
    }
    let n = docs.len().max(1) as f64;
    order
        .into_iter()
        .map(|path| {
            let a = accs.remove(&path).unwrap_or_default();
            let mut types: Vec<(String, u64)> = a.types.into_iter().map(|(t, c)| (t.to_string(), c)).collect();
            types.sort_by(|x, y| y.1.cmp(&x.1));
            let distinct = a.values.len() as u64;
            let mut top: Vec<(Value, u64)> = a.values.into_values().collect();
            top.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.to_string().cmp(&y.0.to_string())));
            top.truncate(5);
            FieldStat {
                presence: a.count as f64 / n,
                count: a.count,
                types,
                top,
                distinct,
                distinct_capped: a.capped,
                min: a.min.map(|(_, b)| to_json(&b)),
                max: a.max.map(|(_, b)| to_json(&b)),
                path,
            }
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSample {
    pub sampled: u64,
    pub total: u64,
    pub fields: Vec<FieldStat>,
}

/// Infer the collection's shape from a `$sample` (after the filter, if any).
pub async fn schema_sample(m: &MongoConn, full: &str, filter: &str, size: i64) -> Result<SchemaSample, String> {
    let (db, coll) = split(full)?;
    let filter = parse_filter(filter)?;
    let c = m.client.database(db).collection::<Document>(coll);
    let size = size.clamp(1, 10_000);
    let mut pipeline = vec![];
    if !filter.is_empty() {
        pipeline.push(doc! { "$match": filter.clone() });
    }
    pipeline.push(doc! { "$sample": { "size": size } });
    let docs: Vec<Document> = c.aggregate(pipeline).await.map_err(err)?.try_collect().await.map_err(err)?;
    let total = if filter.is_empty() { c.estimated_document_count().await } else { c.count_documents(filter).await }.map_err(err)?;
    Ok(SchemaSample { sampled: docs.len() as u64, total, fields: infer_schema(&docs) })
}

// ── Indexes ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDetail {
    pub name: String,
    /// [field, direction] pairs: 1, -1, or the kind ("text", "2dsphere", "hashed").
    pub keys: Vec<(String, Value)>,
    pub unique: bool,
    pub sparse: bool,
    pub hidden: bool,
    pub ttl_seconds: Option<i64>,
    pub partial_filter: Option<Value>,
    /// Operations that used it since `since` ($indexStats), when allowed.
    pub ops: Option<i64>,
    pub since: Option<Value>,
    pub size: Option<f64>,
}

fn as_f64(b: &Bson) -> Option<f64> {
    match b {
        Bson::Int32(i) => Some(*i as f64),
        Bson::Int64(i) => Some(*i as f64),
        Bson::Double(f) => Some(*f),
        _ => None,
    }
}

pub async fn index_details(m: &MongoConn, full: &str) -> Result<Vec<IndexDetail>, String> {
    let (db, coll) = split(full)?;
    let database = m.client.database(db);
    let c = database.collection::<Document>(coll);
    let res = database.run_command(doc! { "listIndexes": coll }).await.map_err(err)?;
    let specs: Vec<Document> = res
        .get_document("cursor")
        .and_then(|cur| cur.get_array("firstBatch"))
        .map(|a| a.iter().filter_map(|x| x.as_document().cloned()).collect())
        .unwrap_or_default();
    // Usage and sizes are extras: skip them when not permitted.
    let mut usage = std::collections::HashMap::new();
    if let Ok(cur) = c.aggregate(vec![doc! { "$indexStats": {} }]).await {
        let stats: Vec<Document> = cur.try_collect().await.unwrap_or_default();
        for s in stats {
            let name = s.get_str("name").unwrap_or_default().to_string();
            let acc = s.get_document("accesses").ok();
            let ops = acc.and_then(|a| a.get("ops")).and_then(as_f64).map(|f| f as i64);
            let since = acc.and_then(|a| a.get("since")).map(to_json);
            usage.insert(name, (ops, since));
        }
    }
    let sizes = database.run_command(doc! { "collStats": coll }).await.ok().and_then(|s| s.get_document("indexSizes").ok().cloned());
    Ok(specs
        .into_iter()
        .map(|s| {
            let name = s.get_str("name").unwrap_or_default().to_string();
            let keys = s.get_document("key").map(|k| k.iter().map(|(f, d)| (f.clone(), to_json(d))).collect()).unwrap_or_default();
            let (ops, since) = usage.remove(&name).unwrap_or((None, None));
            IndexDetail {
                keys,
                unique: s.get_bool("unique").unwrap_or(false) || name == "_id_",
                sparse: s.get_bool("sparse").unwrap_or(false),
                hidden: s.get_bool("hidden").unwrap_or(false),
                ttl_seconds: s.get("expireAfterSeconds").and_then(as_f64).map(|f| f as i64),
                partial_filter: s.get_document("partialFilterExpression").ok().map(doc_to_json),
                ops,
                since,
                size: sizes.as_ref().and_then(|z| z.get(&name)).and_then(as_f64),
                name,
            }
        })
        .collect())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NewIndexOptions {
    pub name: Option<String>,
    pub unique: bool,
    pub sparse: bool,
    pub hidden: bool,
    pub ttl_seconds: Option<i64>,
    /// JSON5 / shell syntax.
    pub partial_filter: Option<String>,
}

/// The server's default name: `field_dir` pairs joined by `_`.
fn default_index_name(keys: &Document) -> String {
    keys.iter()
        .map(|(f, d)| match d {
            Bson::String(s) => format!("{f}_{s}"),
            Bson::Double(x) => format!("{f}_{}", *x as i64),
            other => format!("{f}_{other}"),
        })
        .collect::<Vec<_>>()
        .join("_")
}

pub async fn create_index(m: &MongoConn, full: &str, keys: &str, opts: NewIndexOptions) -> Result<String, String> {
    let (db, coll) = split(full)?;
    let keys = parse_object(keys, "index keys")?;
    if keys.is_empty() {
        return Err("Add at least one key field".into());
    }
    let name = opts.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| default_index_name(&keys));
    let mut spec = doc! { "key": keys, "name": name.clone() };
    if opts.unique {
        spec.insert("unique", true);
    }
    if opts.sparse {
        spec.insert("sparse", true);
    }
    if opts.hidden {
        spec.insert("hidden", true);
    }
    if let Some(t) = opts.ttl_seconds {
        if t < 0 {
            return Err("TTL seconds can't be negative".into());
        }
        spec.insert("expireAfterSeconds", t);
    }
    if let Some(p) = opts.partial_filter.as_deref().filter(|p| !p.trim().is_empty()) {
        spec.insert("partialFilterExpression", parse_object(p, "partial filter")?);
    }
    m.client
        .database(db)
        .run_command(doc! { "createIndexes": coll, "indexes": [spec] })
        .await
        .map_err(|e| format!("Couldn't create the index: {e}"))?;
    Ok(name)
}

pub async fn drop_index(m: &MongoConn, full: &str, name: &str) -> Result<(), String> {
    let (db, coll) = split(full)?;
    if name == "_id_" {
        return Err("The _id index can't be dropped".into());
    }
    m.client
        .database(db)
        .run_command(doc! { "dropIndexes": coll, "index": name })
        .await
        .map_err(|e| format!("Couldn't drop the index: {e}"))?;
    Ok(())
}

// ── One field ────────────────────────────────────────────────────────────────

fn get_path<'a>(d: &'a Document, path: &str) -> Option<&'a Bson> {
    let mut cur = d;
    let mut parts = path.split('.').peekable();
    while let Some(p) = parts.next() {
        let v = cur.get(p)?;
        if parts.peek().is_none() {
            return Some(v);
        }
        cur = v.as_document()?;
    }
    None
}

/// A cell's new value as BSON. `type_hint` is the grid's type for the cell
/// (objectId, date, long, decimal, uuid…); the stored value's type wins for
/// plain numbers.
pub fn typed_value(v: &Value, type_hint: Option<&str>, stored: Option<&Bson>) -> Result<Bson, String> {
    let s = v.as_str();
    Ok(match (type_hint.unwrap_or(""), v) {
        (_, Value::Null) => Bson::Null,
        ("objectId", Value::String(x)) => Bson::ObjectId(mongodb::bson::oid::ObjectId::parse_str(x.trim()).map_err(|_| format!("\"{x}\" isn't an ObjectId (24 hex characters)"))?),
        ("date", Value::String(x)) => {
            let t = x.trim();
            let iso = if t.len() == 10 { format!("{t}T00:00:00Z") } else { t.replacen(' ', "T", 1) };
            let dt = chrono::DateTime::parse_from_rfc3339(&iso)
                .map(|d| d.timestamp_millis())
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(&iso, "%Y-%m-%dT%H:%M:%S%.f").map(|d| d.and_utc().timestamp_millis()))
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(&iso, "%Y-%m-%dT%H:%M").map(|d| d.and_utc().timestamp_millis()))
                .map_err(|_| format!("\"{x}\" isn't a date (use 2026-01-31T12:00:00Z)"))?;
            Bson::DateTime(mongodb::bson::DateTime::from_millis(dt))
        }
        ("date", Value::Number(n)) => Bson::DateTime(mongodb::bson::DateTime::from_millis(n.as_i64().ok_or("A date is milliseconds since 1970")?)),
        ("long", _) => {
            let txt = s.map(str::trim).map(str::to_string).unwrap_or_else(|| v.to_string());
            Bson::Int64(txt.parse::<i64>().map_err(|_| format!("\"{txt}\" isn't a whole number"))?)
        }
        ("decimal", _) => {
            let txt = s.map(str::trim).map(str::to_string).unwrap_or_else(|| v.to_string());
            Bson::Decimal128(txt.parse().map_err(|_| format!("\"{txt}\" isn't a decimal"))?)
        }
        ("uuid", Value::String(x)) => Bson::try_from(json!({ "$uuid": x.trim() })).map_err(|_| format!("\"{x}\" isn't a UUID"))?,
        _ => to_bson(v, stored)?,
    })
}

/// `$set` (or `$unset`) one field of one document; returns the stored document.
pub async fn update_field(m: &MongoConn, full: &str, id: &Value, path: &str, value: &Value, type_hint: Option<&str>, unset: bool) -> Result<Value, String> {
    let (db, coll) = split(full)?;
    let path = path.trim();
    if path.is_empty() || path == "_id" || path.starts_with("_id.") {
        return Err("_id can't be edited in place; open the document to change it".into());
    }
    if path.starts_with('$') {
        return Err("Field names can't start with $".into());
    }
    let c = m.client.database(db).collection::<Document>(coll);
    let id = to_bson(id, None)?;
    let stored = c.find_one(doc! { "_id": id.clone() }).await.map_err(err)?.ok_or("This document no longer exists; reload and try again")?;
    let update = if unset {
        doc! { "$unset": { path: "" } }
    } else {
        doc! { "$set": { path: typed_value(value, type_hint, get_path(&stored, path))? } }
    };
    c.update_one(doc! { "_id": id.clone() }, update).await.map_err(err)?;
    let back = c.find_one(doc! { "_id": id }).await.map_err(err)?.ok_or("The document could not be read back")?;
    Ok(doc_to_json(&back))
}

// ── Aggregate ────────────────────────────────────────────────────────────────

/// A pipeline in shell syntax: `[{$match: {...}}, ...]`.
pub fn parse_pipeline(src: &str) -> Result<Vec<Document>, String> {
    let v: Value = json5::from_str(&shell_to_json5(src.trim())).map_err(|e| format!("The pipeline isn't valid: {e}"))?;
    let Value::Array(stages) = v else { return Err("A pipeline is an array of stages".into()) };
    stages
        .iter()
        .enumerate()
        .map(|(i, s)| match s {
            Value::Object(o) if o.len() == 1 => to_doc(s, None),
            _ => Err(format!("Stage {} has one operator, e.g. {{$match: {{...}}}}", i + 1)),
        })
        .collect()
}

/// Run a pipeline and return at most `limit` documents. `$out` / `$merge`
/// write, so they are left to the console.
pub async fn aggregate_preview(m: &MongoConn, full: &str, pipeline: &str, limit: i64) -> Result<Vec<Value>, String> {
    let (db, coll) = split(full)?;
    let mut stages = parse_pipeline(pipeline)?;
    if stages.iter().any(|s| s.contains_key("$out") || s.contains_key("$merge")) {
        return Err("$out and $merge write data; run them from the console".into());
    }
    stages.push(doc! { "$limit": limit.clamp(1, 5_000) });
    let docs: Vec<Document> = m.client.database(db).collection::<Document>(coll).aggregate(stages).await.map_err(err)?.try_collect().await.map_err(err)?;
    Ok(docs.iter().map(doc_to_json).collect())
}

// ── Import ───────────────────────────────────────────────────────────────────

/// One CSV record list (RFC 4180: quotes, doubled quotes, newlines in quotes).
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut in_q = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_q {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_q = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => in_q = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].trim().is_empty()));
    rows
}

/// A CSV cell as JSON: number, bool, null, ISO date (as `$date`), else string.
fn csv_value(s: &str, infer: bool) -> Value {
    if !infer {
        return Value::String(s.to_string());
    }
    let t = s.trim();
    if t.is_empty() || t == "null" || t == "NULL" {
        return Value::Null;
    }
    match t {
        "true" | "TRUE" | "True" => return Value::Bool(true),
        "false" | "FALSE" | "False" => return Value::Bool(false),
        _ => {}
    }
    // No leading zeros ("007" is an id, not 7).
    let digits = t.trim_start_matches('-');
    let lead_zero = digits.len() > 1 && digits.starts_with('0') && !digits.starts_with("0.");
    if !lead_zero {
        if let Ok(i) = t.parse::<i64>() {
            return json!(i);
        }
        if t.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')) {
            if let Ok(f) = t.parse::<f64>() {
                if f.is_finite() {
                    return json!(f);
                }
            }
        }
    }
    let b = t.as_bytes();
    if t.len() >= 10 && b[4] == b'-' && b[7] == b'-' && b[..4].iter().all(u8::is_ascii_digit) {
        let iso = if t.len() == 10 { format!("{t}T00:00:00Z") } else { t.replacen(' ', "T", 1) };
        if chrono::DateTime::parse_from_rfc3339(&iso).is_ok() {
            return json!({ "$date": iso });
        }
    }
    Value::String(s.to_string())
}

fn set_dotted(o: &mut serde_json::Map<String, Value>, key: &str, v: Value) {
    match key.split_once('.') {
        Some((head, rest)) if !head.is_empty() && !rest.is_empty() => {
            let e = o.entry(head.to_string()).or_insert_with(|| Value::Object(Default::default()));
            if !e.is_object() {
                *e = Value::Object(Default::default());
            }
            if let Value::Object(inner) = e {
                set_dotted(inner, rest, v);
            }
        }
        _ => {
            o.insert(key.to_string(), v);
        }
    }
}

/// Parse an import file's text. `format` is json / jsonl / csv, or empty to
/// detect it from `name` and the content. Returns the format used.
pub fn parse_import(name: &str, text: &str, format: &str, infer: bool) -> Result<(String, Vec<Value>), String> {
    let text = text.trim_start_matches('\u{feff}');
    let lower = name.to_ascii_lowercase();
    let fmt = match format {
        "json" | "jsonl" | "csv" => format.to_string(),
        _ if lower.ends_with(".csv") || lower.ends_with(".tsv") => "csv".into(),
        _ if lower.ends_with(".jsonl") || lower.ends_with(".ndjson") => "jsonl".into(),
        _ => {
            let t = text.trim_start();
            if t.starts_with('[') {
                "json".into()
            } else if t.starts_with('{') {
                // One object, or one per line.
                if serde_json::from_str::<Value>(t).is_ok() { "json".into() } else { "jsonl".into() }
            } else {
                "csv".into()
            }
        }
    };
    let docs = match fmt.as_str() {
        "json" => match serde_json::from_str::<Value>(text).map_err(|e| format!("Not valid JSON: {e}"))? {
            Value::Array(a) => a,
            v @ Value::Object(_) => vec![v],
            _ => return Err("The file holds neither a document nor an array of them".into()),
        },
        "jsonl" => text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(i, l)| serde_json::from_str::<Value>(l).map_err(|e| format!("Line {}: {e}", i + 1)))
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            let mut rows = parse_csv(text).into_iter();
            let header = rows.next().ok_or("The CSV file is empty")?;
            let header: Vec<String> = header.iter().map(|h| h.trim().to_string()).collect();
            rows.map(|r| {
                let mut o = serde_json::Map::new();
                for (i, h) in header.iter().enumerate().filter(|(_, h)| !h.is_empty()) {
                    set_dotted(&mut o, h, csv_value(r.get(i).map(String::as_str).unwrap_or(""), infer));
                }
                Value::Object(o)
            })
            .collect()
        }
    };
    if let Some(i) = docs.iter().position(|d| !d.is_object()) {
        return Err(format!("Item {} isn't a document", i + 1));
    }
    Ok((fmt, docs))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub file_name: String,
    pub format: String,
    pub total: usize,
    pub preview: Vec<Value>,
}

const IMPORT_MAX_BYTES: u64 = 256 * 1024 * 1024;

fn read_import(path: &str) -> Result<(String, String), String> {
    let p = std::path::Path::new(path);
    let len = std::fs::metadata(p).map_err(|e| format!("Couldn't read the file: {e}"))?.len();
    if len > IMPORT_MAX_BYTES {
        return Err("The file is over 256 MB; use mongoimport for files this size".into());
    }
    let text = std::fs::read_to_string(p).map_err(|e| format!("Couldn't read the file: {e}"))?;
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Ok((name, text))
}

pub fn import_preview(path: &str, format: &str, infer: bool) -> Result<ImportPreview, String> {
    let (name, text) = read_import(path)?;
    let (format, docs) = parse_import(&name, &text, format, infer)?;
    Ok(ImportPreview { file_name: name, format, total: docs.len(), preview: docs.into_iter().take(5).collect() })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub inserted: u64,
    pub failed: u64,
    pub errors: Vec<String>,
}

/// Insert a file's documents in unordered batches of 1,000.
pub async fn import_file(m: &MongoConn, full: &str, path: &str, format: &str, infer: bool) -> Result<ImportResult, String> {
    let (db, coll) = split(full)?;
    let (name, text) = read_import(path)?;
    let (_, docs) = parse_import(&name, &text, format, infer)?;
    drop(text);
    let c = m.client.database(db).collection::<Document>(coll);
    let mut res = ImportResult { inserted: 0, failed: 0, errors: vec![] };
    for (b, chunk) in docs.chunks(1000).enumerate() {
        let batch = chunk.iter().map(|d| to_doc(d, None)).collect::<Result<Vec<_>, _>>()?;
        let n = batch.len() as u64;
        match c.insert_many(batch).ordered(false).await {
            Ok(r) => res.inserted += r.inserted_ids.len() as u64,
            Err(e) => match *e.kind {
                mongodb::error::ErrorKind::InsertMany(ref ime) => {
                    let wes = ime.write_errors.as_deref().unwrap_or(&[]);
                    res.failed += wes.len() as u64;
                    res.inserted += n.saturating_sub(wes.len() as u64);
                    for we in wes.iter().take(5usize.saturating_sub(res.errors.len())) {
                        res.errors.push(format!("Document {}: {}", b * 1000 + we.index + 1, dup_key(we.message.clone())));
                    }
                }
                _ => return Err(format!("Import stopped after {} documents: {e}", res.inserted)),
            },
        }
    }
    Ok(res)
}

// ── Console ──────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct Call {
    pub name: String,
    pub args: String,
}

#[derive(Debug, PartialEq)]
pub enum Stmt {
    Use(String),
    ShowDbs,
    ShowCollections,
    /// `db.<coll>.<method>(...)[.sort(...)][.limit(...)][.skip(...)]`
    Coll { coll: String, calls: Vec<Call> },
}

/// Split `.name(args).name(args)` into calls, honoring strings and nesting.
fn calls(mut s: &str) -> Result<Vec<Call>, String> {
    let mut out = vec![];
    while !s.trim().is_empty() {
        let t = s.trim_start().strip_prefix('.').ok_or("Expected .method(...)")?;
        let open = t.find('(').ok_or("Expected .method(...)")?;
        let name = t[..open].trim().to_string();
        let (mut depth, mut in_str, mut esc, mut close) = (0i32, None::<char>, false, None);
        for (i, c) in t[open..].char_indices() {
            if let Some(q) = in_str {
                if esc { esc = false } else if c == '\\' { esc = true } else if c == q { in_str = None }
                continue;
            }
            match c {
                '"' | '\'' => in_str = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close.ok_or("Unbalanced brackets")?;
        out.push(Call { name, args: t[open + 1..close].to_string() });
        s = &t[close + 1..];
    }
    Ok(out)
}

const METHODS: &[&str] = &[
    "find", "findOne", "aggregate", "countDocuments", "estimatedDocumentCount", "distinct", "insertOne", "insertMany",
    "updateOne", "updateMany", "replaceOne", "deleteOne", "deleteMany", "getIndexes", "explain",
];

pub fn parse(stmt: &str) -> Result<Stmt, String> {
    let s = stmt.trim().trim_end_matches(';').trim();
    let usage = "Use db.<collection>.find({...}), .aggregate([...]), .countDocuments({...}), .insertOne({...}), .updateOne(...), .deleteOne(...), or `use <db>` / `show collections`";
    if let Some(db) = s.strip_prefix("use ") {
        return Ok(Stmt::Use(db.trim().to_string()));
    }
    match s {
        "show dbs" | "show databases" => return Ok(Stmt::ShowDbs),
        "show collections" | "show tables" => return Ok(Stmt::ShowCollections),
        _ => {}
    }
    let rest = s.strip_prefix("db.").ok_or(usage)?;
    // db.getCollection("name").method(...)
    if let Some(r) = rest.strip_prefix("getCollection(") {
        let r = r.trim_start();
        let q = r.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or("getCollection() takes a quoted name")?;
        let end = r[1..].find(q).ok_or("Unclosed collection name")? + 1;
        let name = r[1..end].to_string();
        let after = r[end + 1..].trim_start().strip_prefix(')').ok_or(usage)?;
        return Ok(Stmt::Coll { coll: name, calls: calls(after)? });
    }
    // The collection name runs up to the first `.method(` (names may hold dots).
    let at = METHODS
        .iter()
        .filter_map(|m| rest.find(&format!(".{m}(")).map(|i| (i, m)))
        .min_by_key(|(i, _)| *i)
        .ok_or(usage)?;
    let coll = rest[..at.0].trim().to_string();
    if coll.is_empty() {
        return Err(usage.into());
    }
    Ok(Stmt::Coll { coll, calls: calls(&rest[at.0..])? })
}

fn arg_doc(args: &[Value], i: usize, what: &str) -> Result<Document, String> {
    match args.get(i) {
        None => Ok(Document::new()),
        Some(v) if v.is_object() => to_doc(v, None),
        Some(_) => Err(format!("The {what} is an object")),
    }
}

const CONSOLE_CAP: i64 = 5_000;

pub async fn query(m: &MongoConn, stmt: &str) -> Result<super::query::SqlResult, String> {
    let t0 = std::time::Instant::now();
    let db_name = m.current_db.lock().map(|g| g.clone()).unwrap_or_default();
    let one = |label: &str, v: Value| (vec![json!({ label: v })], None::<String>);
    let (docs, message): (Vec<Value>, Option<String>) = match parse(stmt)? {
        Stmt::Use(db) => {
            if let Ok(mut g) = m.current_db.lock() {
                *g = db.clone();
            }
            (vec![], Some(format!("Switched to db {db}")))
        }
        Stmt::ShowDbs => {
            let names = m.client.list_database_names().await.map_err(err)?;
            (names.into_iter().map(|n| json!({ "name": n })).collect(), None)
        }
        Stmt::ShowCollections => {
            let mut names = m.client.database(&db_name).list_collection_names().await.map_err(err)?;
            names.sort();
            (names.into_iter().map(|n| json!({ "name": n })).collect(), None)
        }
        Stmt::Coll { coll, calls } => {
            let c = m.client.database(&db_name).collection::<Document>(&coll);
            // db.c.explain().find(...) and db.c.find(...).explain() both explain.
            let mut calls = calls;
            let mut explain = calls.first().is_some_and(|c| c.name == "explain");
            if explain {
                calls.remove(0);
            }
            let (head, mods) = calls.split_first().ok_or("Missing method")?;
            let args = parse_args(&head.args)?;
            let (mut sort, mut limit, mut skip) = (None, None, None);
            for md in mods {
                let a = parse_args(&md.args)?;
                match md.name.as_str() {
                    "sort" => sort = Some(arg_doc(&a, 0, "sort")?),
                    "limit" => limit = a.first().and_then(|v| v.as_i64()),
                    "skip" => skip = a.first().and_then(|v| v.as_u64()),
                    "explain" => explain = true,
                    "pretty" | "toArray" => {}
                    other => return Err(format!(".{other}() isn't supported here")),
                }
            }
            if explain {
                let cmd = match head.name.as_str() {
                    "find" => {
                        let mut f = doc! { "find": coll.as_str(), "filter": arg_doc(&args, 0, "filter")? };
                        if let Some(p) = args.get(1).filter(|v| v.is_object()) {
                            f.insert("projection", to_doc(p, None)?);
                        }
                        if let Some(s) = sort {
                            f.insert("sort", s);
                        }
                        if let Some(l) = limit {
                            f.insert("limit", l);
                        }
                        if let Some(k) = skip {
                            f.insert("skip", k as i64);
                        }
                        f
                    }
                    "aggregate" => {
                        let pipeline = match args.first() {
                            Some(Value::Array(stages)) => stages.iter().map(|s| to_doc(s, None)).collect::<Result<Vec<_>, _>>()?,
                            _ => return Err("aggregate() takes an array of stages".into()),
                        };
                        doc! { "aggregate": coll.as_str(), "pipeline": pipeline, "cursor": {} }
                    }
                    "countDocuments" => doc! { "count": coll.as_str(), "query": arg_doc(&args, 0, "filter")? },
                    "distinct" => {
                        let field = args.first().and_then(|v| v.as_str()).ok_or("distinct() takes a field name")?;
                        doc! { "distinct": coll.as_str(), "key": field, "query": arg_doc(&args, 1, "filter")? }
                    }
                    other => return Err(format!("{other}() can't be explained here; try find() or aggregate()")),
                };
                let raw = m.client.database(&db_name).run_command(doc! { "explain": cmd, "verbosity": "executionStats" }).await.map_err(err)?;
                let sum = summarize_explain(&raw);
                let rows = sum
                    .stages
                    .iter()
                    .map(|s| {
                        json!({
                            "stage": format!("{}{}", "  ".repeat(s.depth as usize), s.stage),
                            "index": s.index_name,
                            "keysExamined": s.keys_examined,
                            "docsExamined": s.docs_examined,
                            "nReturned": s.n_returned,
                            "ms": s.ms,
                        })
                    })
                    .chain(std::iter::once(json!({ "stage": "(raw explain)", "raw": sum.raw })))
                    .collect();
                return Ok(super::couchdb::docs_result(rows, Some(sum.verdict), stmt, t0));
            }
            match head.name.as_str() {
                "find" => {
                    let mut q = c.find(arg_doc(&args, 0, "filter")?).limit(limit.unwrap_or(CONSOLE_CAP).min(CONSOLE_CAP));
                    if let Some(p) = args.get(1).filter(|v| v.is_object()) {
                        q = q.projection(to_doc(p, None)?);
                    }
                    if let Some(s) = sort {
                        q = q.sort(s);
                    }
                    if let Some(k) = skip {
                        q = q.skip(k);
                    }
                    let docs: Vec<Document> = q.await.map_err(err)?.try_collect().await.map_err(err)?;
                    (docs.iter().map(doc_to_json).collect(), None)
                }
                "findOne" => {
                    let d = c.find_one(arg_doc(&args, 0, "filter")?).await.map_err(err)?;
                    (d.iter().map(doc_to_json).collect(), d.is_none().then(|| "No document matched".to_string()))
                }
                "aggregate" => {
                    let pipeline = match args.first() {
                        Some(Value::Array(stages)) => stages.iter().map(|s| to_doc(s, None)).collect::<Result<Vec<_>, _>>()?,
                        _ => return Err("aggregate() takes an array of stages".into()),
                    };
                    let docs: Vec<Document> = c.aggregate(pipeline).await.map_err(err)?.try_collect().await.map_err(err)?;
                    (docs.iter().take(CONSOLE_CAP as usize).map(doc_to_json).collect(), None)
                }
                "countDocuments" => one("count", json!(c.count_documents(arg_doc(&args, 0, "filter")?).await.map_err(err)?)),
                "estimatedDocumentCount" => one("count", json!(c.estimated_document_count().await.map_err(err)?)),
                "distinct" => {
                    let field = args.first().and_then(|v| v.as_str()).ok_or("distinct() takes a field name")?;
                    let vals = c.distinct(field, arg_doc(&args, 1, "filter")?).await.map_err(err)?;
                    (vals.iter().map(|v| json!({ field: to_json(v) })).collect(), None)
                }
                "getIndexes" => {
                    let idx: Vec<_> = c.list_indexes().await.map_err(err)?.try_collect().await.map_err(err)?;
                    (idx.iter().map(|i| json!({ "name": i.options.as_ref().and_then(|o| o.name.clone()), "key": doc_to_json(&i.keys), "unique": i.options.as_ref().and_then(|o| o.unique) })).collect(), None)
                }
                "insertOne" => {
                    let r = c.insert_one(arg_doc(&args, 0, "document")?).await.map_err(|e| dup_key(e.to_string()))?;
                    (vec![json!({ "insertedId": to_json(&r.inserted_id) })], Some("Inserted 1 document".into()))
                }
                "insertMany" => {
                    let docs = match args.first() {
                        Some(Value::Array(a)) => a.iter().map(|d| to_doc(d, None)).collect::<Result<Vec<_>, _>>()?,
                        _ => return Err("insertMany() takes an array of documents".into()),
                    };
                    let r = c.insert_many(docs).await.map_err(|e| dup_key(e.to_string()))?;
                    (vec![], Some(format!("Inserted {} documents", r.inserted_ids.len())))
                }
                "updateOne" | "updateMany" => {
                    let (f, u) = (arg_doc(&args, 0, "filter")?, arg_doc(&args, 1, "update")?);
                    if !u.keys().all(|k| k.starts_with('$')) {
                        return Err("The update uses operators like {$set: {...}}; to replace a whole document use replaceOne".into());
                    }
                    let r = if head.name == "updateOne" { c.update_one(f, u).await } else { c.update_many(f, u).await }.map_err(err)?;
                    (vec![], Some(format!("Matched {}, modified {}", r.matched_count, r.modified_count)))
                }
                "replaceOne" => {
                    let r = c.replace_one(arg_doc(&args, 0, "filter")?, arg_doc(&args, 1, "replacement")?).await.map_err(err)?;
                    (vec![], Some(format!("Matched {}, modified {}", r.matched_count, r.modified_count)))
                }
                "deleteOne" | "deleteMany" => {
                    let f = arg_doc(&args, 0, "filter")?;
                    if head.name == "deleteMany" && f.is_empty() {
                        return Err("deleteMany({}) would empty the collection; pass a filter".into());
                    }
                    let r = if head.name == "deleteOne" { c.delete_one(f).await } else { c.delete_many(f).await }.map_err(err)?;
                    (vec![], Some(format!("Deleted {}", r.deleted_count)))
                }
                other => return Err(format!("{other}() isn't supported here")),
            }
        }
    };
    Ok(super::couchdb::docs_result(docs, message, stmt, t0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_keys_read_as_fields() {
        let id = index_from_keys("users", "_id_".into(), &doc! {"_id": 1}, false);
        assert!(id.is_primary && id.is_unique && id.columns == "_id" && id.index_type == "btree");
        let compound = index_from_keys("users", "plan_age".into(), &doc! {"plan": 1, "age": -1}, true);
        assert_eq!(compound.columns, "plan, age desc");
        assert!(compound.is_unique && !compound.is_primary);
        let text = index_from_keys("posts", "body_text".into(), &doc! {"body": "text"}, false);
        assert_eq!((text.columns.as_str(), text.index_type.as_str()), ("body text", "text"));
    }

    #[test]
    fn shell_literals() {
        assert_eq!(
            parse_args(r#"{_id: ObjectId("65a1b2c3d4e5f60718293a4b"), at: {$gte: ISODate("2026-01-01")}, n: NumberLong(5), i: NumberInt(3)}"#).unwrap(),
            vec![json!({"_id": {"$oid": "65a1b2c3d4e5f60718293a4b"}, "at": {"$gte": {"$date": "2026-01-01T00:00:00Z"}}, "n": {"$numberLong": "5"}, "i": 3})]
        );
        // Helper names inside strings are left alone.
        assert_eq!(parse_args(r#"{note: "call ObjectId(x) later"}"#).unwrap(), vec![json!({"note": "call ObjectId(x) later"})]);
        assert_eq!(parse_args("{a: 1}, {b: -1}").unwrap().len(), 2);
    }

    #[test]
    fn json_bson_round_trip_keeps_types() {
        let stored = doc! {
            "_id": mongodb::bson::oid::ObjectId::parse_str("65a1b2c3d4e5f60718293a4b").unwrap(),
            "small_long": 5i64, "big_long": 9_007_199_254_740_993i64, "i": 3i32, "d": 2.0f64,
            "at": mongodb::bson::DateTime::from_millis(1_768_384_800_000),
            "dec": mongodb::bson::Decimal128::from_bytes([1; 16]),
            "nested": { "xs": [1i64, 2i64] },
        };
        let j = doc_to_json(&stored);
        assert_eq!(j["small_long"], json!(5));
        assert_eq!(j["big_long"], json!({"$numberLong": "9007199254740993"}));
        assert_eq!(j["_id"], json!({"$oid": "65a1b2c3d4e5f60718293a4b"}));
        let back = to_doc(&j, Some(&stored)).unwrap();
        assert_eq!(back, stored);
        // Without a hint, small ints are Int32 and doubles stay doubles.
        assert_eq!(to_doc(&json!({"a": 1, "b": 1.5}), None).unwrap(), doc! {"a": 1i32, "b": 1.5});
    }

    #[test]
    fn console_parses() {
        let s = parse(r#"db.users.find({plan: "pro"}).sort({age: -1}).limit(10)"#).unwrap();
        let Stmt::Coll { coll, calls } = s else { panic!() };
        assert_eq!(coll, "users");
        assert_eq!(calls.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["find", "sort", "limit"]);
        let Stmt::Coll { coll, .. } = parse(r#"db.system.profile.find({})"#).unwrap() else { panic!() };
        assert_eq!(coll, "system.profile");
        let Stmt::Coll { coll, .. } = parse(r#"db.getCollection("a.b(c)").countDocuments({})"#).unwrap() else { panic!() };
        assert_eq!(coll, "a.b(c)");
        assert_eq!(parse("use shop;").unwrap(), Stmt::Use("shop".into()));
        assert_eq!(parse("show collections").unwrap(), Stmt::ShowCollections);
        assert!(parse("SELECT 1").is_err());
        assert!(parse(r#"db.users.find({a: "x)"}).limit(2)"#).is_ok(), "a ) inside a string");
    }

    #[test]
    fn hex_strings_also_match_object_ids() {
        let hex = "6af49eec8add90c8ab59139f";
        let oid = mongodb::bson::oid::ObjectId::parse_str(hex).unwrap();
        let f = parse_filter(&format!(r#"{{userId: "{hex}", $or: [{{a: {{$ne: "{hex}"}}}}, {{b: {{$in: ["{hex}", "x"]}}}}], c: {{$regex: "{hex}"}}, d: ObjectId("{hex}"), e: "short"}}"#)).unwrap();
        assert_eq!(f.get("userId").unwrap(), &Bson::Document(doc! { "$in": [hex, oid] }));
        let or = f.get_array("$or").unwrap();
        assert_eq!(or[0].as_document().unwrap().get("a").unwrap(), &Bson::Document(doc! { "$nin": [hex, oid] }));
        assert_eq!(or[1].as_document().unwrap().get("b").unwrap(), &Bson::Document(doc! { "$in": [hex, oid, "x"] }));
        assert_eq!(f.get("c").unwrap(), &Bson::Document(doc! { "$regex": hex }));
        assert_eq!(f.get("d").unwrap(), &Bson::ObjectId(oid));
        assert_eq!(f.get_str("e").unwrap(), "short");
    }

    #[test]
    fn schema_inference() {
        let docs = vec![
            doc! { "name": "a", "age": 30i32, "tags": ["x", "y"], "addr": { "city": "Oslo" } },
            doc! { "name": "b", "age": 40i64, "tags": ["x"] },
            doc! { "name": "a", "age": "n/a" },
            doc! { "name": "c" },
        ];
        let f = infer_schema(&docs);
        let get = |p: &str| f.iter().find(|x| x.path == p).unwrap_or_else(|| panic!("{p}"));
        assert_eq!(f[0].path, "name");
        assert_eq!(get("name").presence, 1.0);
        assert_eq!(get("name").top[0], (json!("a"), 2));
        let age = get("age");
        assert_eq!((age.count, age.types.len()), (3, 3));
        assert_eq!((age.min.clone(), age.max.clone()), (Some(json!(30)), Some(json!(40))));
        assert_eq!(get("tags").types, vec![("array".to_string(), 2)]);
        assert_eq!((get("tags[]").count, get("tags[]").top[0].clone()), (2, (json!("x"), 2)));
        assert_eq!(get("addr.city").presence, 0.25);
    }

    #[test]
    fn import_csv_infers_types() {
        let (fmt, docs) = parse_import("p.csv", "name,n,ok,at,zip,addr.city\n\"A, b\",3,true,2026-01-02,007,Oslo\nC,1.5,,x,1,\n", "", true).unwrap();
        assert_eq!(fmt, "csv");
        assert_eq!(docs[0], json!({"name": "A, b", "n": 3, "ok": true, "at": {"$date": "2026-01-02T00:00:00Z"}, "zip": "007", "addr": {"city": "Oslo"}}));
        assert_eq!(docs[1]["n"], json!(1.5));
        assert_eq!(docs[1]["ok"], Value::Null);
        let (fmt, docs) = parse_import("x.json", "{\"a\":1}\n{\"a\":2}\n", "", true).unwrap();
        assert_eq!((fmt.as_str(), docs.len()), ("jsonl", 2));
    }

    /// Against dev/nosql MongoDB (port 27017): `cargo test --lib mongo_live -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn mongo_live() {
        let m = connect(&MongoConfig { name: "t".into(), uri: "mongodb://localhost:27017".into(), database: Some("shop".into()) }).await.unwrap();
        let colls = list_collections(&m).await.unwrap();
        assert!(colls.contains(&"shop.users".to_string()), "{colls:?}");
        let info = describe(&m, "shop.orders").await.unwrap();
        assert_eq!(info.count, 1200);
        let p = find_page_with(&m, "shop.orders", "", "{_id: 1}", "", 0, 100).await.unwrap();
        assert_eq!((p.docs.len(), p.next), (100, Some(100)));
        let f = find_page_with(&m, "shop.users", r#"{plan: "pro"}"#, "", "", 0, 100).await.unwrap();
        assert!(!f.docs.is_empty() && f.docs.iter().all(|d| d["plan"] == "pro"));
        // Asha's balance is a Decimal128 and visits an Int64 past 2^53.
        let asha = find_page_with(&m, "shop.users", r#"{name: "Asha"}"#, "", "", 0, 1).await.unwrap().docs.remove(0);
        assert!(asha["balance"].get("$numberDecimal").is_some(), "{}", asha["balance"]);
        assert_eq!(asha["visits"], json!({"$numberLong": "9007199254740993"}));
        // Edit through JSON: types survive.
        let mut edited = asha.clone();
        edited["plan"] = json!("team");
        let saved = save(&m, "shop.users", &edited, Some(&asha["_id"])).await.unwrap();
        assert_eq!(saved["plan"], "team");
        assert_eq!(saved["visits"], asha["visits"]);
        assert_eq!(saved["createdAt"], asha["createdAt"]);
        save(&m, "shop.users", &asha, Some(&asha["_id"])).await.unwrap();
        // Create (generated id), duplicate, move, delete.
        let created = save(&m, "shop.users", &json!({"_id": "", "name": "IT"}), None).await.unwrap();
        assert!(created["_id"].get("$oid").is_some());
        let err = save(&m, "shop.users", &json!({"_id": created["_id"], "name": "dup"}), None).await.unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        let moved = save(&m, "shop.users", &json!({"_id": "it-moved", "name": "IT"}), Some(&created["_id"])).await.unwrap();
        assert_eq!(moved["_id"], "it-moved");
        assert!(delete(&m, "shop.users", &created["_id"]).await.is_err(), "old id is gone");
        delete(&m, "shop.users", &json!("it-moved")).await.unwrap();
        // Console.
        let r = query(&m, r#"db.orders.find({}).sort({_id: -1}).limit(3)"#).await.unwrap();
        assert_eq!(r.rows.len(), 3);
        let r = query(&m, r#"db.orders.aggregate([{$group: {_id: "$status", n: {$sum: 1}}}])"#).await.unwrap();
        assert!(!r.rows.is_empty());
        let r = query(&m, "db.users.countDocuments({})").await.unwrap();
        assert_eq!(r.columns[0].name, "count");
        assert!(query(&m, "db.users.deleteMany({})").await.unwrap_err().contains("would empty"));
        // Explain, schema, indexes, preview.
        let ex = explain(&m, "shop.orders", r#"{status: "paid"}"#, "", "").await.unwrap();
        assert!(ex.collscan && ex.verdict.starts_with("Collection scan"), "{}", ex.verdict);
        let ex = explain(&m, "shop.orders", &format!(r#"{{userId: "{}"}}"#, asha["_id"]["$oid"].as_str().unwrap()), "", "").await.unwrap();
        assert!(!ex.collscan, "{}", ex.verdict);
        let sc = schema_sample(&m, "shop.orders", "", 1000).await.unwrap();
        assert_eq!(sc.sampled, 1000);
        assert!(sc.fields.iter().any(|f| f.path == "items[].sku"));
        assert!(index_details(&m, "shop.orders").await.unwrap().iter().any(|i| i.name == "_id_"));
        assert_eq!(aggregate_preview(&m, "shop.orders", "[{$match: {}}]", 3).await.unwrap().len(), 3);
        // One field, index create/drop, import.
        let up = update_field(&m, "shop.users", &asha["_id"], "plan", &json!("team"), Some("string"), false).await.unwrap();
        assert_eq!(up["plan"], "team");
        update_field(&m, "shop.users", &asha["_id"], "plan", &asha["plan"], None, false).await.unwrap();
        assert!(update_field(&m, "shop.users", &asha["_id"], "_id", &json!(1), None, false).await.is_err());
        let ix = create_index(&m, "shop.orders", "{status: 1}", NewIndexOptions { sparse: true, ..Default::default() }).await.unwrap();
        assert_eq!(ix, "status_1");
        assert!(index_details(&m, "shop.orders").await.unwrap().iter().any(|i| i.name == "status_1" && i.sparse));
        drop_index(&m, "shop.orders", "status_1").await.unwrap();
        let tmp = std::env::temp_dir().join("stroke_import_test.csv");
        std::fs::write(&tmp, "name,n\nimp-a,1\nimp-b,2\n").unwrap();
        let r = import_file(&m, "shop.import_test", tmp.to_str().unwrap(), "", true).await.unwrap();
        assert_eq!((r.inserted, r.failed), (2, 0));
        m.client.database("shop").collection::<Document>("import_test").drop().await.unwrap();
        let r = query(&m, r#"db.orders.find({status: "paid"}).explain()"#).await.unwrap();
        assert!(r.rows.len() >= 2);
        query(&m, "use admin").await.unwrap();
        assert_eq!(*m.current_db.lock().unwrap(), "admin");
    }
}
