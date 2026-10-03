//! Console results kept in the backend, read by the window as it scrolls.
//!
//! A 5,011,000-row `SELECT *` sent to the window whole held about 2GB there and
//! made scrolling stutter on garbage collection. Here the rows stream into a
//! spool file as they arrive (one JSON array per line) with an offset per row
//! in memory: 8 bytes a row, 40MB for those 5M, against the 2.8GB the backend
//! held as values. The window keeps only the rows near its viewport and asks
//! for a window of rows when it scrolls to them (`read_window`).
//!
//! The reading side follows the shape of a streaming engine like Epsio's: the
//! query is an append-only stream of rows, and everything derived from it is
//! kept up to date as rows land instead of recomputed: the readable row count
//! moves with every flush, so the window can show and scroll rows while the
//! rest are still arriving. Sorting is the exception, it needs every row, so it
//! runs once the stream is done (`sort`), typed by the column's database type:
//! numbers by value, timestamps and dates by instant, text naturally.

use super::query::ColumnInfo;
use serde_json::Value;
use std::cmp::Ordering;
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex, RwLock};

/// Results kept at once. A new run in a tab drops that tab's last one, so this
/// only bounds what a window that forgot to drop could leave behind on disk.
const MAX_RESULTS: usize = 16;

/// Every stored result, by the run id the console sent with its query.
pub struct ResultStore {
    results: Mutex<Vec<(String, Arc<StoredResult>)>>,
    dir: PathBuf,
}

impl Default for ResultStore {
    fn default() -> Self {
        let dir = std::env::temp_dir().join("stroke-results");
        let _ = std::fs::create_dir_all(&dir);
        remove_other_spools(&dir);
        Self { results: Mutex::new(Vec::new()), dir }
    }
}

/// Spool files this process did not make: left behind by a process that was
/// killed (`tauri dev` restarts the app on every Rust change) or crashed, so
/// it never removed them. Removing one that another running instance still
/// reads is harmless: its open handle keeps the data until it closes.
fn remove_other_spools(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mine = format!("{}-", std::process::id());
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(&mine) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

impl ResultStore {
    /// A new, empty result for run `id`, replacing any earlier one with that id.
    pub fn create(&self, id: &str) -> Result<SpoolWriter, String> {
        let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
        let path = self.dir.join(format!("{}-{safe}.jsonl", std::process::id()));
        let write = File::create(&path).map_err(|e| format!("Could not create the result file: {e}"))?;
        let read = File::open(&path).map_err(|e| format!("Could not open the result file: {e}"))?;
        let result = Arc::new(StoredResult {
            path,
            read,
            columns: RwLock::new(Vec::new()),
            offsets: RwLock::new(Vec::new()),
            end: AtomicU64::new(0),
            order: RwLock::new(None),
            done: AtomicBool::new(false),
        });
        let mut results = self.results.lock().unwrap();
        results.retain(|(k, _)| k != id);
        if results.len() >= MAX_RESULTS {
            results.remove(0);
        }
        results.push((id.to_string(), result.clone()));
        drop(results);
        Ok(SpoolWriter { file: BufWriter::with_capacity(1 << 20, write), pos: 0, pending: Vec::new(), scratch: Vec::new(), result })
    }

    pub fn get(&self, id: &str) -> Option<Arc<StoredResult>> {
        self.results.lock().unwrap().iter().find(|(k, _)| k == id).map(|(_, r)| r.clone())
    }

    /// Forget a result; its file goes with the last reference to it.
    pub fn drop_result(&self, id: &str) {
        self.results.lock().unwrap().retain(|(k, _)| k != id);
    }
}

/// One result: its spool file, where each readable row starts, and an
/// optional sort order over those rows.
pub struct StoredResult {
    path: PathBuf,
    read: File,
    columns: RwLock<Vec<ColumnInfo>>,
    /// Start of every readable row. Grows on each flush; a row is listed only
    /// once its bytes are on disk.
    offsets: RwLock<Vec<u64>>,
    /// End of the last readable row.
    end: AtomicU64,
    /// Row numbers in display order, once sorted.
    order: RwLock<Option<Vec<u32>>>,
    done: AtomicBool,
}

impl Drop for StoredResult {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl StoredResult {
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.offsets.read().unwrap().len()
    }

    pub fn is_done(&self) -> bool {
        self.done.load(AtomicOrdering::Acquire)
    }

    /// Rows `[start, start + count)` in display order, as the text of one JSON
    /// array of row arrays, ready to hand to the window without re-encoding.
    pub fn read_window(&self, start: usize, count: usize) -> Result<String, String> {
        let offsets = self.offsets.read().unwrap();
        let n = offsets.len();
        let start = start.min(n);
        let stop = start.saturating_add(count).min(n);
        let end_of = |row: usize| if row + 1 < n { offsets[row + 1] } else { self.end.load(AtomicOrdering::Acquire) };
        let mut out = Vec::with_capacity(2 + (stop - start) * 64);
        out.push(b'[');
        let order = self.order.read().unwrap();
        match order.as_deref() {
            None if stop > start => {
                // In stream order the window is one contiguous read.
                let from = offsets[start];
                let mut buf = vec![0u8; (end_of(stop - 1) - from) as usize];
                read_at(&self.read, &mut buf, from)?;
                // Rows end in '\n': the separators become commas, the last one goes.
                buf.pop();
                for b in buf.iter_mut() {
                    if *b == b'\n' {
                        *b = b',';
                    }
                }
                out.extend_from_slice(&buf);
            }
            None => {}
            Some(order) => {
                let mut buf = Vec::new();
                for (i, &row) in order[start..stop].iter().enumerate() {
                    let row = row as usize;
                    let from = offsets[row];
                    buf.resize((end_of(row) - from) as usize, 0);
                    read_at(&self.read, &mut buf, from)?;
                    if i > 0 {
                        out.push(b',');
                    }
                    out.extend_from_slice(&buf[..buf.len() - 1]);
                }
            }
        }
        out.push(b']');
        String::from_utf8(out).map_err(|e| format!("The result file is corrupt: {e}"))
    }

    /// Sort by column `column` (descending when `desc`), or back to stream
    /// order with `None`. Reads every row once, so it waits for the stream.
    ///
    /// Keys are pulled straight from each row's raw JSON (only the sort cell
    /// is decoded) on several threads, one stretch of rows each. Measured on
    /// 5M rows in a debug build: decoding whole rows and parsing timestamps
    /// with chrono's format parser took 17s.
    pub fn sort(&self, column: Option<usize>, desc: bool) -> Result<(), String> {
        let Some(column) = column else {
            *self.order.write().unwrap() = None;
            return Ok(());
        };
        if !self.is_done() {
            return Err("Sorting is available once every row has arrived.".into());
        }
        let data_type = self
            .columns
            .read()
            .unwrap()
            .get(column)
            .map(|c| c.data_type.clone())
            .ok_or_else(|| format!("No column {column} in this result."))?;
        let kind = KeyKind::of(&data_type);
        let offsets = self.offsets.read().unwrap();
        let n = offsets.len();
        let end = self.end.load(AtomicOrdering::Acquire);
        let threads = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4).clamp(1, 8);
        let per = n.div_ceil(threads).max(1);
        // One list, each thread filling its own stretch of it: building a list
        // per thread and joining them held every key twice at the peak.
        let mut keys: Vec<SortKey> = Vec::new();
        keys.resize_with(n, || SortKey::Null);
        let results: Vec<Result<(), String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = keys
                .chunks_mut(per)
                .enumerate()
                .map(|(i, out)| {
                    let offsets = &offsets[..];
                    let file = &self.read;
                    scope.spawn(move || keys_for_rows(file, offsets, end, i * per, out, column, kind))
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("A sort thread failed.".into()))).collect()
        });
        drop(offsets);
        for r in results {
            r?;
        }
        let mut order: Vec<u32> = (0..n as u32).collect();
        // Stable, so equal keys keep their stream order. NULLs last either way.
        order.sort_by(|&a, &b| {
            let (ka, kb) = (&keys[a as usize], &keys[b as usize]);
            match (ka.is_null(), kb.is_null()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ if desc => kb.cmp(ka),
                _ => ka.cmp(kb),
            }
        });
        *self.order.write().unwrap() = Some(order);
        Ok(())
    }
}

/// Sort keys of rows `[from, from + out.len())` into `out`, read in blocks of
/// about 4MB.
fn keys_for_rows(
    file: &File,
    offsets: &[u64],
    end: u64,
    from: usize,
    out: &mut [SortKey],
    column: usize,
    kind: KeyKind,
) -> Result<(), String> {
    const BLOCK: u64 = 4 << 20;
    let end_of = |row: usize| if row + 1 < offsets.len() { offsets[row + 1] } else { end };
    let to = from + out.len();
    let mut keys = out.iter_mut();
    let mut buf = Vec::new();
    let mut row = from;
    while row < to {
        // As many whole rows as fit in a block (at least one).
        let start = offsets[row];
        let mut last = row;
        while last + 1 < to && end_of(last + 1) - start <= BLOCK {
            last += 1;
        }
        buf.resize((end_of(last) - start) as usize, 0);
        read_at(file, &mut buf, start)?;
        for line in buf.split(|&b| b == b'\n').take(last - row + 1) {
            if let Some(slot) = keys.next() {
                *slot = SortKey::of_raw(nth_cell(line, column), kind);
            }
        }
        row = last + 1;
    }
    Ok(())
}

/// The raw JSON of the `k`th element of a one-line JSON array, without
/// decoding the others. Rows are written by serde_json, so the text is valid.
fn nth_cell(line: &[u8], k: usize) -> Option<&[u8]> {
    let mut i = line.iter().position(|&b| b == b'[')? + 1;
    let mut index = 0;
    let mut depth = 0usize;
    let mut cell_start = i;
    let mut in_str = false;
    while i < line.len() {
        let b = line[i];
        if in_str {
            match b {
                b'\\' => i += 1,
                b'"' => in_str = false,
                _ => {}
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'[' | b'{' => depth += 1,
                b']' | b'}' if depth > 0 => depth -= 1,
                b',' | b']' if depth == 0 => {
                    if index == k {
                        // A JSON cell is never empty: an empty one is `[]`.
                        let cell = trim_ascii(&line[cell_start..i]);
                        return (!cell.is_empty()).then_some(cell);
                    }
                    if b == b']' {
                        return None;
                    }
                    index += 1;
                    cell_start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

fn trim_ascii(b: &[u8]) -> &[u8] {
    let start = b.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(b.len());
    let end = b.iter().rposition(|c| !c.is_ascii_whitespace()).map_or(start, |e| e + 1);
    &b[start..end]
}

/// Positioned read, so concurrent windows never share a cursor.
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(buf, offset).map_err(|e| format!("Could not read the result file: {e}"))
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0;
        while done < buf.len() {
            let n = file
                .seek_read(&mut buf[done..], offset + done as u64)
                .map_err(|e| format!("Could not read the result file: {e}"))?;
            if n == 0 {
                return Err("The result file ended early.".into());
            }
            done += n;
        }
        Ok(())
    }
}

/// The writing side of one result, owned by the query's row loop.
pub struct SpoolWriter {
    file: BufWriter<File>,
    pos: u64,
    /// Starts of the rows written since the last flush, not yet readable.
    pending: Vec<u64>,
    scratch: Vec<u8>,
    result: Arc<StoredResult>,
}

impl SpoolWriter {
    /// The result starts (again, after a retry re-runs the query).
    pub fn start(&mut self, columns: Vec<ColumnInfo>) -> Result<(), String> {
        self.file.flush().map_err(|e| e.to_string())?;
        let f = self.file.get_mut();
        f.set_len(0).map_err(|e| e.to_string())?;
        f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        self.pos = 0;
        self.pending.clear();
        self.result.offsets.write().unwrap().clear();
        self.result.end.store(0, AtomicOrdering::Release);
        *self.result.columns.write().unwrap() = columns;
        Ok(())
    }

    /// Append one row; it becomes readable at the next `flush`.
    pub fn push(&mut self, row: &[Value]) -> Result<usize, String> {
        self.scratch.clear();
        serde_json::to_writer(&mut self.scratch, row).map_err(|e| e.to_string())?;
        self.scratch.push(b'\n');
        self.file.write_all(&self.scratch).map_err(|e| format!("Could not write the result file: {e}"))?;
        self.pending.push(self.pos);
        self.pos += self.scratch.len() as u64;
        Ok(self.scratch.len())
    }

    /// Make every row written so far readable. Returns how many rows are.
    pub fn flush(&mut self) -> Result<usize, String> {
        self.file.flush().map_err(|e| format!("Could not write the result file: {e}"))?;
        let mut offsets = self.result.offsets.write().unwrap();
        offsets.append(&mut self.pending);
        self.result.end.store(self.pos, AtomicOrdering::Release);
        Ok(offsets.len())
    }

    /// The stream ended (or was stopped): what is written is the whole result.
    pub fn finish(&mut self) -> Result<usize, String> {
        let n = self.flush()?;
        self.result.done.store(true, AtomicOrdering::Release);
        Ok(n)
    }
}

// ── Typed sort keys ──────────────────────────────────────────────────────────

/// How a column's values compare, from its database type.
#[derive(Clone, Copy, Debug, PartialEq)]
enum KeyKind {
    Integer,
    Float,
    /// Exact decimal text (`numeric`): compared digit by digit, never through
    /// a float that would round 20-digit values together.
    Decimal,
    /// Timestamps, dates and times: compared as instants. As text they sorted
    /// wrong whenever fractional seconds had different lengths.
    Temporal,
    Boolean,
    /// Anything else: by the JSON value, text compared naturally.
    Other,
}

impl KeyKind {
    fn of(data_type: &str) -> Self {
        // `_int4` and friends are arrays: they sort as text (Other).
        let t = data_type.to_ascii_lowercase();
        match t.as_str() {
            "int2" | "int4" | "int8" | "oid" | "smallint" | "integer" | "bigint" | "tinyint" | "mediumint"
            | "int" | "year" => Self::Integer,
            "float4" | "float8" | "real" | "double" | "double precision" | "float" => Self::Float,
            "numeric" | "decimal" | "money" => Self::Decimal,
            "timestamptz" | "timestamp" | "date" | "time" | "datetime" => Self::Temporal,
            "bool" | "boolean" => Self::Boolean,
            _ => Self::Other,
        }
    }
}

/// One cell, reduced to what ordering needs. Variants are ordered by rank, so
/// a column that mixes kinds (text where a number was expected) still has one
/// total order.
/// 24 bytes, so 5M of them are 120MB for the length of a sort.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SortKey {
    Bool(bool),
    Int(i64),
    Float(OrdF64),
    Decimal(Box<DecimalText>),
    /// Microseconds since the epoch, or since midnight for a time of day.
    Time(i64),
    Text(NaturalText),
    Null,
}

impl SortKey {
    fn is_null(&self) -> bool {
        matches!(self, SortKey::Null)
    }

    /// From a cell's raw JSON. Plain strings and numbers skip the decoder.
    fn of_raw(raw: Option<&[u8]>, kind: KeyKind) -> Self {
        let Some(raw) = raw else { return SortKey::Null };
        match raw.first() {
            None => SortKey::Null,
            Some(b'n') if raw == b"null" => SortKey::Null,
            Some(b'"') if raw.len() >= 2 && !raw.contains(&b'\\') => {
                // No escapes: the text between the quotes is the string.
                match std::str::from_utf8(&raw[1..raw.len() - 1]) {
                    Ok(text) => Self::of_str(text, kind),
                    Err(_) => SortKey::Null,
                }
            }
            Some(b'-' | b'0'..=b'9') if matches!(kind, KeyKind::Integer | KeyKind::Float | KeyKind::Other) => {
                let text = std::str::from_utf8(raw).unwrap_or("");
                match text.parse::<i64>() {
                    Ok(i) if kind != KeyKind::Float => SortKey::Int(i),
                    _ => SortKey::Float(OrdF64(text.parse().unwrap_or(f64::NAN))),
                }
            }
            _ => match serde_json::from_slice::<Value>(raw) {
                Ok(v) => Self::of(&v, kind),
                Err(_) => SortKey::Null,
            },
        }
    }

    fn of_str(s: &str, kind: KeyKind) -> Self {
        let text = || SortKey::Text(NaturalText(s.into()));
        match kind {
            KeyKind::Temporal => fast_temporal_micros(s).or_else(|| temporal_micros(s)).map(SortKey::Time).unwrap_or_else(text),
            KeyKind::Decimal => DecimalText::parse(s).map(|d| SortKey::Decimal(Box::new(d))).unwrap_or_else(text),
            KeyKind::Integer => s.trim().parse().map(SortKey::Int).unwrap_or_else(|_| text()),
            KeyKind::Float => match s.trim() {
                "NaN" => SortKey::Float(OrdF64(f64::NAN)),
                "Infinity" => SortKey::Float(OrdF64(f64::INFINITY)),
                "-Infinity" => SortKey::Float(OrdF64(f64::NEG_INFINITY)),
                t => t.parse().map(|f| SortKey::Float(OrdF64(f))).unwrap_or_else(|_| text()),
            },
            KeyKind::Boolean | KeyKind::Other => text(),
        }
    }

    /// From a decoded cell: strings with escapes, arrays, objects, booleans.
    fn of(cell: &Value, kind: KeyKind) -> Self {
        match cell {
            Value::Null => SortKey::Null,
            Value::Bool(b) => SortKey::Bool(*b),
            Value::String(s) => Self::of_str(s, kind),
            Value::Number(n) => match (kind, n.as_i64()) {
                (KeyKind::Decimal, _) => DecimalText::parse(&n.to_string())
                    .map(|d| SortKey::Decimal(Box::new(d)))
                    .unwrap_or(SortKey::Null),
                (KeyKind::Float, _) | (_, None) => SortKey::Float(OrdF64(n.as_f64().unwrap_or(f64::NAN))),
                (_, Some(i)) => SortKey::Int(i),
            },
            other => SortKey::Text(NaturalText(other.to_string().into())),
        }
    }
}

/// A float with a total order (NaN above everything, as Postgres sorts it).
#[derive(Debug, PartialEq)]
struct OrdF64(f64);
impl Eq for OrdF64 {}
impl PartialOrd for OrdF64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrdF64 {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.0.is_nan(), other.0.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => self.0.partial_cmp(&other.0).unwrap_or(Ordering::Equal),
        }
    }
}

/// An exact decimal from its text: sign, integer digits without leading zeros,
/// fraction digits without trailing zeros. `NaN` and the infinities rank
/// around every finite value, as in Postgres.
#[derive(Debug, PartialEq, Eq)]
struct DecimalText {
    /// -2 = -Infinity, -1 = negative, 0 = zero, 1 = positive, 2 = Infinity, 3 = NaN.
    class: i8,
    int: Box<str>,
    frac: Box<str>,
}

impl DecimalText {
    fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let special = |class| Some(Self { class, int: "".into(), frac: "".into() });
        match s {
            "NaN" => return special(3),
            "Infinity" | "+Infinity" => return special(2),
            "-Infinity" => return special(-2),
            _ => {}
        }
        let (neg, digits) = match s.as_bytes().first()? {
            b'-' => (true, &s[1..]),
            b'+' => (false, &s[1..]),
            _ => (false, s),
        };
        let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
        if int.is_empty() && frac.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let int = int.trim_start_matches('0');
        let frac = frac.trim_end_matches('0');
        let class = if int.is_empty() && frac.is_empty() { 0 } else if neg { -1 } else { 1 };
        Some(Self { class, int: int.into(), frac: frac.into() })
    }

    fn cmp_magnitude(&self, other: &Self) -> Ordering {
        self.int
            .len()
            .cmp(&other.int.len())
            .then_with(|| self.int.cmp(&other.int))
            .then_with(|| self.frac.cmp(&other.frac))
    }
}

impl PartialOrd for DecimalText {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for DecimalText {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.class.cmp(&other.class) {
            Ordering::Equal => match self.class {
                1 => self.cmp_magnitude(other),
                -1 => other.cmp_magnitude(self),
                _ => Ordering::Equal,
            },
            o => o,
        }
    }
}

/// Text that compares the way people read it: case-insensitively, runs of
/// digits by value (`file2` before `file10`), then byte order to break ties.
#[derive(Debug, PartialEq, Eq)]
struct NaturalText(Box<str>);

impl PartialOrd for NaturalText {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for NaturalText {
    fn cmp(&self, other: &Self) -> Ordering {
        natural_cmp(&self.0, &other.0).then_with(|| self.0.cmp(&other.0))
    }
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut run = String::new();
                    while let Some(c) = it.peek().copied().filter(|c| c.is_ascii_digit()) {
                        run.push(c);
                        it.next();
                    }
                    run
                };
                let (ra, rb) = (take(&mut a), take(&mut b));
                let (ta, tb) = (ra.trim_start_matches('0'), rb.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// `YYYY-MM-DD[ T]HH:MM:SS[.fraction][ UTC|Z|±HH[:MM]]` or `YYYY-MM-DD`, read
/// by hand: the common shapes of temporal cells, without chrono's format
/// machinery on every row. Anything else falls through to `temporal_micros`.
fn fast_temporal_micros(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let part = b.get(from..from + len)?;
        part.iter().try_fold(0i64, |acc, &c| c.is_ascii_digit().then(|| acc * 10 + (c - b'0') as i64))
    };
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let (y, m, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let mut micros = days_from_civil(y, m, d) * 86_400_000_000;
    if b.len() == 10 {
        return Some(micros);
    }
    if !(b[10] == b' ' || b[10] == b'T') || b.len() < 19 || b[13] != b':' || b[16] != b':' {
        return None;
    }
    micros += (num(11, 2)? * 3600 + num(14, 2)? * 60 + num(17, 2)?) * 1_000_000;
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let digits = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        let frac = &b[i..i + digits];
        let mut us = 0i64;
        for k in 0..6 {
            us = us * 10 + frac.get(k).map_or(0, |c| (c - b'0') as i64);
        }
        micros += us;
        i += digits;
    }
    match &s[i..] {
        "" | " UTC" | "Z" => Some(micros),
        zone => {
            let zone = zone.trim_start();
            let sign = match zone.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let digits: Vec<u8> = zone[1..].bytes().filter(|c| *c != b':').collect();
            if digits.len() != 2 && digits.len() != 4 || !digits.iter().all(u8::is_ascii_digit) {
                return None;
            }
            let h = ((digits[0] - b'0') * 10 + (digits[1] - b'0')) as i64;
            let mi = if digits.len() == 4 { ((digits[2] - b'0') * 10 + (digits[3] - b'0')) as i64 } else { 0 };
            Some(micros - sign * (h * 60 + mi) * 60_000_000)
        }
    }
}

/// Days from 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Microseconds for the temporal text cells come as: `2024-01-02 10:00:00.5 UTC`
/// (timestamptz), `2024-01-02 10:00:00` (timestamp), `2024-01-02` (date),
/// `10:00:00` (time), ISO `T`-separated or offset forms from other engines.
fn temporal_micros(s: &str) -> Option<i64> {
    use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime};
    let s = s.trim();
    let s = s.strip_suffix(" UTC").unwrap_or(s);
    for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(t) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(t.and_utc().timestamp_micros());
        }
    }
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Some(t.timestamp_micros());
    }
    for fmt in ["%Y-%m-%d %H:%M:%S%.f%#z", "%Y-%m-%d %H:%M:%S%.f%:z"] {
        if let Ok(t) = DateTime::parse_from_str(s, fmt) {
            return Some(t.timestamp_micros());
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.and_hms_opt(0, 0, 0)?.and_utc().timestamp_micros());
    }
    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M:%S%.f") {
        let midnight = NaiveTime::from_hms_opt(0, 0, 0)?;
        return (t - midnight).num_microseconds();
    }
    None
}

/// For tests elsewhere that check an order without reaching into SortKey.
#[cfg(test)]
pub(crate) mod tests_support {
    pub fn micros(v: &serde_json::Value) -> i64 {
        super::temporal_micros(v.as_str().unwrap_or_default()).unwrap_or(i64::MIN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store_with(rows: &[Value], types: &[&str]) -> (ResultStore, Arc<StoredResult>) {
        let store = ResultStore::default();
        let id = format!("test-{}", rand_id());
        let mut w = store.create(&id).unwrap();
        w.start(types.iter().enumerate().map(|(i, t)| ColumnInfo::new(format!("c{i}"), *t)).collect()).unwrap();
        for r in rows {
            w.push(r.as_array().unwrap()).unwrap();
        }
        w.finish().unwrap();
        let r = store.get(&id).unwrap();
        (store, r)
    }

    /// Unique per test in this process: tests run in parallel, and two that
    /// shared a spool path truncated each other's file.
    fn rand_id() -> u64 {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        NEXT.fetch_add(1, AtomicOrdering::Relaxed)
    }

    fn window(r: &StoredResult, start: usize, count: usize) -> Vec<Value> {
        serde_json::from_str::<Vec<Value>>(&r.read_window(start, count).unwrap()).unwrap()
    }

    #[test]
    fn windows_read_back_what_was_written() {
        let rows: Vec<Value> = (0..1000).map(|i| json!([i, format!("row {i}"), null])).collect();
        let (_s, r) = store_with(&rows, &["int4", "text", "text"]);
        assert_eq!(r.len(), 1000);
        assert_eq!(window(&r, 0, 3), rows[0..3].to_vec());
        assert_eq!(window(&r, 997, 50), rows[997..1000].to_vec());
        assert_eq!(window(&r, 2000, 10), Vec::<Value>::new());
        assert_eq!(r.read_window(1000, 5).unwrap(), "[]");
    }

    #[test]
    fn rows_are_readable_only_after_a_flush() {
        let store = ResultStore::default();
        let mut w = store.create("flush-test").unwrap();
        w.start(vec![ColumnInfo::new("a", "int4")]).unwrap();
        w.push(&[json!(1)]).unwrap();
        let r = store.get("flush-test").unwrap();
        assert_eq!(r.len(), 0);
        assert_eq!(w.flush().unwrap(), 1);
        assert_eq!(window(&r, 0, 10), vec![json!([1])]);
        assert!(!r.is_done());
        assert!(r.sort(Some(0), false).is_err(), "sorting a half-streamed result");
    }

    #[test]
    fn a_retry_starts_the_result_over() {
        let store = ResultStore::default();
        let mut w = store.create("retry-test").unwrap();
        w.start(vec![ColumnInfo::new("a", "int4")]).unwrap();
        w.push(&[json!(1)]).unwrap();
        w.flush().unwrap();
        w.start(vec![ColumnInfo::new("a", "text")]).unwrap();
        w.push(&[json!("x")]).unwrap();
        w.finish().unwrap();
        let r = store.get("retry-test").unwrap();
        assert_eq!(window(&r, 0, 10), vec![json!(["x"])]);
    }

    #[test]
    fn dropping_a_result_removes_its_file() {
        let store = ResultStore::default();
        let mut w = store.create("drop-test").unwrap();
        w.start(vec![ColumnInfo::new("a", "int4")]).unwrap();
        w.finish().unwrap();
        let path = store.get("drop-test").unwrap().path.clone();
        assert!(path.exists());
        drop(w);
        store.drop_result("drop-test");
        assert!(!path.exists());
    }

    fn sorted_col(rows: Vec<Value>, ty: &str, desc: bool) -> Vec<Value> {
        let (_s, r) = store_with(&rows, &[ty]);
        r.sort(Some(0), desc).unwrap();
        window(&r, 0, rows.len()).into_iter().map(|row| row[0].clone()).collect()
    }

    #[test]
    fn timestamps_sort_by_instant_whatever_their_fraction_length() {
        // As text with numeric collation, ".500" sorted before ".123456".
        let got = sorted_col(
            vec![json!(["2024-01-02 10:00:00.500 UTC"]), json!(["2024-01-02 10:00:00.123456 UTC"]), json!(["2024-01-02 09:59:59 UTC"])],
            "timestamptz",
            false,
        );
        assert_eq!(got, vec![json!("2024-01-02 09:59:59 UTC"), json!("2024-01-02 10:00:00.123456 UTC"), json!("2024-01-02 10:00:00.500 UTC")]);
        let dates = sorted_col(vec![json!(["2024-10-01"]), json!(["2024-02-29"]), json!([null]), json!(["1999-12-31"])], "date", true);
        assert_eq!(dates, vec![json!("2024-10-01"), json!("2024-02-29"), json!("1999-12-31"), Value::Null]);
    }

    #[test]
    fn numeric_text_sorts_exactly_by_value() {
        let got = sorted_col(
            vec![json!(["10"]), json!(["9.5"]), json!(["-2"]), json!(["12345678901234567890.1"]), json!(["12345678901234567890.01"]), json!(["0.00"]), json!(["-10.5"]), json!(["NaN"])],
            "numeric",
            false,
        );
        assert_eq!(
            got,
            vec![json!("-10.5"), json!("-2"), json!("0.00"), json!("9.5"), json!("10"), json!("12345678901234567890.01"), json!("12345678901234567890.1"), json!("NaN")]
        );
    }

    #[test]
    fn integers_floats_and_text_sort_by_their_own_rules() {
        assert_eq!(sorted_col(vec![json!([10]), json!([9]), json!([null]), json!([-1])], "int8", false), vec![json!(-1), json!(9), json!(10), Value::Null]);
        assert_eq!(sorted_col(vec![json!([1.5]), json!([-0.5]), json!([1e10])], "float8", true), vec![json!(1e10), json!(1.5), json!(-0.5)]);
        assert_eq!(
            sorted_col(vec![json!(["file10"]), json!(["File2"]), json!(["file1"])], "text", false),
            vec![json!("file1"), json!("File2"), json!("file10")]
        );
    }

    #[test]
    fn the_fast_temporal_parser_agrees_with_chrono() {
        for s in [
            "2024-01-02 10:00:00.123456 UTC",
            "2024-01-02 10:00:00 UTC",
            "1969-12-31 23:59:59.5",
            "2024-02-29T08:30:00Z",
            "2024-01-02 10:00:00+05:45",
            "2024-01-02 10:00:00-0300",
            "1999-12-31",
            "0001-01-01 00:00:00",
        ] {
            assert_eq!(fast_temporal_micros(s), temporal_micros(s), "{s}");
        }
        assert_eq!(fast_temporal_micros("10:00:00"), None);
        assert_eq!(fast_temporal_micros("2024-13-01"), None);
    }

    #[test]
    fn nth_cell_skips_strings_and_nesting() {
        let line = br#"[1,"a,b]\"c",[1,[2]],{"k":"]"},null,"x"]"#;
        assert_eq!(nth_cell(line, 0), Some(&b"1"[..]));
        assert_eq!(nth_cell(line, 1), Some(&br#""a,b]\"c""#[..]));
        assert_eq!(nth_cell(line, 2), Some(&b"[1,[2]]"[..]));
        assert_eq!(nth_cell(line, 3), Some(&br#"{"k":"]"}"#[..]));
        assert_eq!(nth_cell(line, 4), Some(&b"null"[..]));
        assert_eq!(nth_cell(line, 5), Some(&br#""x""#[..]));
        assert_eq!(nth_cell(line, 6), None);
        assert_eq!(nth_cell(b"[]", 0), None);
    }

    #[test]
    fn equal_keys_keep_stream_order_and_none_restores_it() {
        let rows = vec![json!([1, "a"]), json!([0, "b"]), json!([1, "c"]), json!([0, "d"])];
        let (_s, r) = store_with(&rows, &["int4", "text"]);
        r.sort(Some(0), false).unwrap();
        let names: Vec<Value> = window(&r, 0, 4).into_iter().map(|row| row[1].clone()).collect();
        assert_eq!(names, vec![json!("b"), json!("d"), json!("a"), json!("c")]);
        r.sort(None, false).unwrap();
        assert_eq!(window(&r, 0, 4), rows);
    }
}
