#!/usr/bin/env bash
# Seed the containers from dev/nosql/docker-compose.yml with test data.
# Safe to re-run: each database is reset first.
set -euo pipefail
cd "$(dirname "$0")"
C="docker compose -f docker-compose.yml"

wait_for() { # name, command
  local name=$1; shift
  for _ in $(seq 1 90); do "$@" >/dev/null 2>&1 && return 0; sleep 2; done
  echo "$name did not come up" >&2; return 1
}

# ── DynamoDB Local (http://localhost:8000, any credentials) ────────────────
echo "DynamoDB…"
export AWS_ACCESS_KEY_ID=local AWS_SECRET_ACCESS_KEY=local AWS_DEFAULT_REGION=us-east-1 AWS_PAGER=""
DDB="aws dynamodb --endpoint-url http://localhost:8000"
wait_for dynamodb $DDB list-tables
for t in stroke-test Orders; do $DDB delete-table --table-name $t >/dev/null 2>&1 || true; done
# Single-table design: users, their orders and sessions under one pk.
$DDB create-table --table-name stroke-test \
  --attribute-definitions AttributeName=pk,AttributeType=S AttributeName=sk,AttributeType=S AttributeName=gsi1pk,AttributeType=S \
  --key-schema AttributeName=pk,KeyType=HASH AttributeName=sk,KeyType=RANGE \
  --global-secondary-indexes 'IndexName=gsi1,KeySchema=[{AttributeName=gsi1pk,KeyType=HASH},{AttributeName=sk,KeyType=RANGE}],Projection={ProjectionType=ALL}' \
  --billing-mode PAY_PER_REQUEST >/dev/null
$DDB create-table --table-name Orders \
  --attribute-definitions AttributeName=orderId,AttributeType=S \
  --key-schema AttributeName=orderId,KeyType=HASH --billing-mode PAY_PER_REQUEST >/dev/null
put() { $DDB put-item --table-name "$1" --item "$2"; }
put stroke-test '{"pk":{"S":"USER#1"},"sk":{"S":"PROFILE"},"gsi1pk":{"S":"PLAN#pro"},"name":{"S":"Asha"},"email":{"S":"asha@example.com"},"age":{"N":"31"},"tags":{"SS":["beta","admin"]},"prefs":{"M":{"theme":{"S":"dark"},"notify":{"BOOL":true}}}}'
put stroke-test '{"pk":{"S":"USER#1"},"sk":{"S":"ORDER#2026-11-01"},"total":{"N":"49.99"},"status":{"S":"shipped"},"items":{"L":[{"M":{"sku":{"S":"A1"},"qty":{"N":"2"}}},{"M":{"sku":{"S":"B7"},"qty":{"N":"1"}}}]}}'
put stroke-test '{"pk":{"S":"USER#1"},"sk":{"S":"ORDER#2026-11-05"},"total":{"N":"12.50"},"status":{"S":"pending"}}'
put stroke-test '{"pk":{"S":"USER#2"},"sk":{"S":"PROFILE"},"gsi1pk":{"S":"PLAN#free"},"name":{"S":"Ravi"},"email":{"S":"ravi@example.com"},"big":{"N":"12345678901234567890.123456789"},"nickname":{"NULL":true}}'
put stroke-test '{"pk":{"S":"USER#2"},"sk":{"S":"SESSION#a1b2"},"ttl":{"N":"1999999999"},"device":{"S":"iPhone"},"blob":{"B":"aGVsbG8gd29ybGQ="}}'
for i in $(seq 1 250); do
  echo "{\"PutRequest\":{\"Item\":{\"orderId\":{\"S\":\"ord-$(printf %04d $i)\"},\"amount\":{\"N\":\"$((i * 7 % 500)).$((i % 100))\"},\"status\":{\"S\":\"$( ((i % 3)) && echo paid || echo refunded )\"},\"customer\":{\"S\":\"cust-$((i % 40))\"}}}}"
done | paste -sd, - | sed 's/^/[/;s/$/]/' > /tmp/stroke-orders.json
# BatchWriteItem takes 25 at a time.
python3 - <<'PY'
import json, subprocess
items = json.load(open('/tmp/stroke-orders.json'))
for i in range(0, len(items), 25):
    subprocess.run(['aws','dynamodb','--endpoint-url','http://localhost:8000','batch-write-item',
                    '--request-items', json.dumps({'Orders': items[i:i+25]})], check=True, capture_output=True)
PY

# ── CouchDB (http://admin:admin@localhost:5984) ─────────────────────────────
echo "CouchDB…"
CDB=http://admin:admin@localhost:5984
wait_for couchdb curl -sf $CDB/_up
curl -s -X PUT $CDB/_users >/dev/null; curl -s -X PUT $CDB/_replicator >/dev/null
for db in products customers; do curl -s -X DELETE $CDB/$db >/dev/null; curl -s -X PUT $CDB/$db >/dev/null; done
curl -s -X POST $CDB/products/_bulk_docs -H 'Content-Type: application/json' -d '{"docs":[
 {"_id":"p-001","type":"product","name":"Keyboard","price":79.0,"stock":12,"tags":["input","usb"],"dims":{"w":44,"h":3}},
 {"_id":"p-002","type":"product","name":"Mouse","price":29.5,"stock":0,"tags":["input"]},
 {"_id":"p-003","type":"product","name":"Monitor 27\"","price":329,"stock":4,"specs":{"hz":144,"panel":"IPS"}},
 {"_id":"p-004","type":"bundle","name":"Desk set","items":["p-001","p-002"],"discount":0.1}
]}' >/dev/null
curl -s -X PUT $CDB/products/_design/catalog -H 'Content-Type: application/json' -d '{
 "views":{"by_price":{"map":"function(doc){ if(doc.price) emit(doc.price, doc.name) }"},
          "stock_total":{"map":"function(doc){ if(doc.stock!=null) emit(doc.type, doc.stock) }","reduce":"_sum"}}}' >/dev/null
curl -s -X POST $CDB/products/_index -H 'Content-Type: application/json' -d '{"index":{"fields":["type","price"]},"name":"type-price"}' >/dev/null
# A document with history and a replication-style conflict.
rev=$(curl -s -X PUT $CDB/customers/c-1 -d '{"name":"Asha","tier":"gold"}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["rev"])')
curl -s -X PUT "$CDB/customers/c-1?rev=$rev" -d '{"name":"Asha","tier":"platinum"}' >/dev/null
curl -s -X POST $CDB/customers/_bulk_docs -H 'Content-Type: application/json' \
  -d "{\"new_edits\":false,\"docs\":[{\"_id\":\"c-1\",\"_rev\":\"2-conflictingrevision0000000000000\",\"name\":\"Asha D.\",\"tier\":\"gold\",\"_revisions\":{\"start\":2,\"ids\":[\"conflictingrevision0000000000000\",\"${rev#1-}\"]}}]}" >/dev/null
curl -s -X PUT $CDB/customers/c-2 -d '{"name":"Ravi","tier":"silver","addresses":[{"city":"Pune"},{"city":"Kathmandu"}]}' >/dev/null

# ── MongoDB (mongodb://localhost:27017) ──────────────────────────────────────
echo "MongoDB…"
wait_for mongodb $C exec -T mongodb mongosh --quiet --eval 'db.runCommand({ping:1})'
$C exec -T mongodb mongosh --quiet shop --eval '
db.dropDatabase();
db.users.insertMany([
  { name: "Asha", email: "asha@example.com", plan: "pro", createdAt: new Date("2026-01-14T10:00:00Z"),
    balance: NumberDecimal("1024.50"), visits: NumberLong("9007199254740993"), address: { city: "Pune", geo: { lat: 18.52, lng: 73.85 } },
    roles: ["admin", "dev"] },
  { name: "Ravi", email: "ravi@example.com", plan: "free", createdAt: new Date(), uuid: UUID(), tags: [] },
  { name: "Mina", plan: "pro", extra: { nested: { deeper: { deepest: [1, [2, [3]]] } } } },
]);
const users = db.users.find().toArray();
const orders = [];
for (let i = 0; i < 1200; i++) {
  orders.push({ userId: users[i % 3]._id, total: NumberDecimal((i * 3.17).toFixed(2)), status: ["paid","shipped","refunded"][i % 3],
    items: Array.from({ length: (i % 4) + 1 }, (_, j) => ({ sku: "SKU-" + j, qty: j + 1 })), placedAt: new Date(Date.now() - i * 3600e3) });
}
db.orders.insertMany(orders);
db.orders.createIndex({ userId: 1, placedAt: -1 });
db.createCollection("events", { capped: true, size: 1 << 20 });
db.events.insertOne({ kind: "login", at: new Date(), raw: BinData(0, "aGVsbG8=") });
print("mongo: " + db.users.countDocuments() + " users, " + db.orders.countDocuments() + " orders");
'

# ── Cassandra (localhost:9042, no auth) ──────────────────────────────────────
echo "Cassandra (slow to start)…"
wait_for cassandra $C exec -T cassandra cqlsh -e 'describe keyspaces'
CQL=$(mktemp)
cat > "$CQL" <<'CQLEOF'
DROP KEYSPACE IF EXISTS shop;
CREATE KEYSPACE shop WITH replication = {'class': 'SimpleStrategy', 'replication_factor': 1};
CREATE TYPE shop.address (city text, zip text);
CREATE TABLE shop.users (user_id uuid PRIMARY KEY, name text, email text, tags set<text>, prefs map<text, text>, home frozen<address>);
CREATE TABLE shop.orders_by_user (user_id uuid, placed_at timestamp, order_id timeuuid, total decimal, status text, items list<text>,
  PRIMARY KEY ((user_id), placed_at, order_id)) WITH CLUSTERING ORDER BY (placed_at DESC, order_id ASC);
INSERT INTO shop.users (user_id, name, email, tags, prefs, home) VALUES (11111111-1111-1111-1111-111111111111, 'Asha', 'asha@example.com', {'beta','admin'}, {'theme':'dark'}, {city:'Pune', zip:'411001'});
INSERT INTO shop.users (user_id, name, email, tags) VALUES (22222222-2222-2222-2222-222222222222, 'Ravi', 'ravi@example.com', {'free'});
CQLEOF
for i in $(seq 1 60); do
  if ((i % 2)); then u=11111111-1111-1111-1111-111111111111; else u=22222222-2222-2222-2222-222222222222; fi
  if ((i % 3)); then st=paid; else st=shipped; fi
  echo "INSERT INTO shop.orders_by_user (user_id, placed_at, order_id, total, status, items) VALUES ($u, '2026-10-$(printf %02d $((i % 28 + 1))) 10:00:00+0000', now(), $((i * 3)).99, '$st', ['sku-$i','sku-$((i+1))']);" >> "$CQL"
done
$C cp "$CQL" cassandra:/tmp/seed.cql
$C exec -T cassandra cqlsh -f /tmp/seed.cql
rm -f "$CQL"

# ── Redis (localhost:6391) ───────────────────────────────────────────────────
echo "Redis…"
R="$C exec -T redis redis-cli"
$R FLUSHALL >/dev/null
$R HSET user:1 name Asha email asha@example.com plan pro >/dev/null
$R HSET user:2 name Ravi email ravi@example.com plan free >/dev/null
$R SET session:ab12 '{"uid":1,"roles":["admin","dev"],"exp":1700000000}' EX 3600 >/dev/null
$R RPUSH queue:jobs j1 j2 j3 j4 j5 j6 j7 j8 >/dev/null
$R SADD tags:popular redis rust svelte tauri >/dev/null
$R ZADD leaderboard 120 asha 95 ravi 300 mina >/dev/null
$R XADD events '*' type signup user 1 >/dev/null
$R EVAL "for i=1,20000 do redis.call('SET','item:'..i,'value-'..i) end for i=1,5000 do redis.call('HSET','bighash','f'..i,i) end return 1" 0 >/dev/null

# ── Valkey (localhost:6392) ──────────────────────────────────────────────────
echo "Valkey…"
V="$C exec -T valkey valkey-cli"
$V FLUSHALL >/dev/null
$V HSET user:1 name Asha plan pro >/dev/null
$V SET greeting "hello from valkey" >/dev/null
$V RPUSH queue:jobs j1 j2 j3 >/dev/null
$V ZADD leaderboard 120 asha 95 ravi >/dev/null

# ── Firestore emulator (http://localhost:8086, project stroke-test) ──────────
echo "Firestore…"
FS="http://localhost:8086/v1/projects/stroke-test/databases/(default)/documents"
wait_for firestore curl -sf http://localhost:8086/
curl -s -X DELETE "http://localhost:8086/emulator/v1/projects/stroke-test/databases/(default)/documents" >/dev/null
fsput() { curl -s -X PATCH "$FS/$1" -H 'Authorization: Bearer owner' -H 'Content-Type: application/json' -d "$2" >/dev/null; }
fsput users/u1 '{"fields":{"name":{"stringValue":"Asha"},"age":{"integerValue":"31"},"plan":{"stringValue":"pro"},"score":{"doubleValue":9.5},"active":{"booleanValue":true},"joined":{"timestampValue":"2026-01-14T10:00:00Z"},"tags":{"arrayValue":{"values":[{"stringValue":"beta"},{"stringValue":"admin"}]}},"address":{"mapValue":{"fields":{"city":{"stringValue":"Pune"},"zip":{"stringValue":"411001"}}}},"home":{"geoPointValue":{"latitude":18.52,"longitude":73.85}},"manager":{"nullValue":null}}}'
fsput users/u2 '{"fields":{"name":{"stringValue":"Ravi"},"age":{"integerValue":"27"},"plan":{"stringValue":"free"},"active":{"booleanValue":false}}}'
fsput users/u1/orders/o1 '{"fields":{"total":{"doubleValue":49.99},"status":{"stringValue":"shipped"}}}'
for i in $(seq 1 150); do fsput "products/p$i" "{\"fields\":{\"name\":{\"stringValue\":\"Product $i\"},\"price\":{\"integerValue\":\"$((i * 3))\"}}}" & done; wait

echo
echo "Ready:"
echo "  DynamoDB  http://localhost:8000      tables: stroke-test (single-table + GSI), Orders (250 items)"
echo "  CouchDB   http://localhost:5984/_utils  admin/admin  dbs: products (views, Mango index), customers (conflict on c-1)"
echo "  MongoDB   mongodb://localhost:27017   db: shop (users, orders x1200, capped events)"
echo "  Cassandra localhost:9042              keyspace: shop (users w/ set/map/UDT, orders_by_user)"
echo "  Firestore http://localhost:8086 (emulator) project stroke-test: users (+orders sub), products x150"
echo "  Valkey    redis://localhost:6392      a few keys of each type"
echo "  Redis     redis://localhost:6391      ~25k keys incl. a 5000-field hash"
