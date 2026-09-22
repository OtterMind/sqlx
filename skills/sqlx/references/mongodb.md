# MongoDB operations

Each command below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command '...'`. Repeat `--command` in the same invocation when commands need to share connection state; they run in order and the first error stops the rest.

A command is one JSON object, the same shape `db.runCommand()` takes. The write helpers `insertOne`, `insertMany`, `updateOne`, `updateMany`, `replaceOne`, `deleteOne` and `deleteMany`, and the read helper `findOne`, are translated to the server commands `insert`, `update`, `delete` and `find` before they are sent, so both spellings work. The first field names the operation and the rest are its parameters, for example `{"find":"users","filter":{"age":{"$gt":30}},"limit":10}`. A single-quoted shell argument is the easiest way to pass one, because the JSON itself contains double quotes. MongoDB has no SQL, so a command document is the statement here.

`--type mongodb` (alias `mongo`) connects to a standalone server, a replica set or a sharded cluster on port 27017 by default. `--database` selects the database the commands run against, and `--username`/`--password` authenticate against `--property authSource=<database>` (`admin` by default). `--tls disable` leaves TLS off; any other value turns it on. Official links target the MongoDB command reference; match the connected server version when you look them up. The documentation root is https://www.mongodb.com/docs/manual/reference/command/.

## 1. Identify the current connection

**Purpose:** Check that the server answers, its version, and the state of the selected database. No placeholders need replacement.

```json
{"ping": 1}
{"buildInfo": 1}
{"dbStats": 1}
```

**Result:** `ping` answers without rows, and the call succeeds. `buildInfo` returns version and build details as one row. `dbStats` returns one row with the collection, object and size counters for the selected database.

**Official documentation:** [ping](https://www.mongodb.com/docs/manual/reference/command/ping/) · [buildInfo](https://www.mongodb.com/docs/manual/reference/command/buildInfo/) · [dbStats](https://www.mongodb.com/docs/manual/reference/command/dbStats/)

## 2. List collections and indexes

**Purpose:** Discover the collections in the database and the indexes of one collection. Replace `collection`.

```json
{"listCollections": 1}
{"listIndexes": "collection"}
```

**Result:** One row per collection with its name and options, and one row per index with its key pattern, name and uniqueness.

**Official documentation:** [listCollections](https://www.mongodb.com/docs/manual/reference/command/listCollections/) · [listIndexes](https://www.mongodb.com/docs/manual/reference/command/listIndexes/)

## 3. Read documents

**Purpose:** Read a bounded set of documents. Replace `collection`, the filter and the field list.

```json
{"find": "collection", "filter": {}, "projection": {"name": 1}, "limit": 5, "sort": {"_id": 1}}
{"findOne": "collection", "filter": {"_id": 1}}
```

**Result:** One row per document, with the union of the top-level fields as columns and `_id` first. `findOne` is the single-document form of `find` and returns at most one row. Nested documents and arrays appear as JSON text in their cell. The driver follows the cursor until it ends or until 10000 documents have been returned, so a `limit` keeps a read small.

**Official documentation:** [find](https://www.mongodb.com/docs/manual/reference/command/find/) · [findOne](https://www.mongodb.com/docs/manual/reference/command/findOne/)

## 4. Count and aggregate

**Purpose:** Count matches, or compute a result without reading every document. Replace `collection` and the pipeline.

```json
{"count": "collection", "query": {}}
{"distinct": "collection", "key": "field"}
{"aggregate": "collection", "pipeline": [{"$group": {"_id": "$field", "total": {"$sum": 1}}}], "cursor": {}}
```

**Result:** `count` returns a single `n` row. `distinct` returns one row per distinct value. `aggregate` returns one row per output document, mapped like `find`.

**Official documentation:** [count](https://www.mongodb.com/docs/manual/reference/command/count/) · [distinct](https://www.mongodb.com/docs/manual/reference/command/distinct/) · [aggregate](https://www.mongodb.com/docs/manual/reference/command/aggregate/)

## 5. Write documents

**Purpose:** Insert documents. Replace `collection` and the document.

```json
{"insertOne": "collection", "document": {"_id": 1, "name": "ada"}}
{"insertMany": "collection", "documents": [{"_id": 2}, {"_id": 3}]}
```

**Result:** No columns, and `affected_rows` reports the number of inserted documents. A duplicate `_id` fails the whole command with `mongodb.<codeName>`, and the earlier inserts of the same batch stay applied.

**Official documentation:** [insert](https://www.mongodb.com/docs/manual/reference/command/insert/)

## 6. Update documents

**Purpose:** Change the fields of matching documents. Replace `collection`, the filter and the update.

```json
{"updateOne": "collection", "filter": {"_id": 1}, "update": {"$set": {"name": "grace"}}}
{"updateMany": "collection", "filter": {"stale": true}, "update": {"$set": {"stale": false}}}
```

**Result:** No columns, and `affected_rows` reports the number of matched documents. Without `upsert: true` an update that matches nothing changes nothing, and the reply then reports zero.

**Official documentation:** [update](https://www.mongodb.com/docs/manual/reference/command/update/)

## 7. Delete documents

**Purpose:** Remove matching documents or a whole collection. Replace `collection` and the filter.

```json
{"deleteOne": "collection", "filter": {"_id": 1}}
{"deleteMany": "collection", "filter": {}}
{"drop": "collection"}
```

**Result:** No columns, with `affected_rows` reporting the number of removed documents. `drop` removes the collection and its indexes and cannot be undone.

**Official documentation:** [delete](https://www.mongodb.com/docs/manual/reference/command/delete/) · [drop](https://www.mongodb.com/docs/manual/reference/command/drop/)

## 8. Create an index

**Purpose:** Add an index that a slow query needs. Replace `collection` and the key pattern.

```json
{"createIndexes": "collection", "indexes": [{"key": {"name": 1}, "name": "name_1"}]}
```

**Result:** No columns and no affected-row count; the reply reports the created index names. Building an index on a large collection takes time on the server.

**Official documentation:** [createIndexes](https://www.mongodb.com/docs/manual/reference/command/createIndexes/)

## 9. Read server status

**Purpose:** Judge load and replication state before a heavy operation. No placeholders need replacement.

```json
{"serverStatus": 1}
{"collStats": "collection"}
```

**Result:** One row for the reply's fields, with nested sections as JSON text; `collStats` returns one row for the collection's size, count and index sizes.

**Official documentation:** [serverStatus](https://www.mongodb.com/docs/manual/reference/command/serverStatus/) · [collStats](https://www.mongodb.com/docs/manual/reference/command/collStats/)

A reply becomes a table by these rules: `find`, `aggregate` and `listCollections` reuse their cursor into rows; write commands report `affected_rows` from `n` with no columns; `findAndModify` maps its `value` document; any other successful reply with `n` reports it the same way. Column values keep exact numbers as strings, dates as RFC 3339 text, object identifiers as their hex string, binary values as base64, and nested documents or arrays as canonical extended JSON text, so no number loses precision and `$oid`, `$date` and `$numberLong` survive inside them. A successful reply that has neither a cursor nor a write count, such as `dbStats` or `serverStatus`, becomes one row made of its own fields, without the bookkeeping fields `ok`, `operationTime` and `$clusterTime`. Write commands are `insert`, `update`, `delete`, `findAndModify`, `create*`, `drop*`, `renameCollection` and anything else that changes data, indexes or collections; `$where`, `$function`, `mapReduce` and `eval` are unknown operations that can do anything, so confirm them with the user and never replay them after an uncertain outcome.
