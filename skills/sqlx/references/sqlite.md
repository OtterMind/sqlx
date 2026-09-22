# SQLite operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `table_name`, `index_name`, and `column_name` with actual identifiers. SQLite accepts double-quoted identifiers, and single-quoted SQL strings have separate escaping rules.

`--type sqlite` (alias `sqlite3`) opens a local database file: pass it as `--database <file>` or `--path <file>`, and the file is created when it does not exist. SQLite has no users, hosts or ports, so those fields stay empty. `--property mode=ro` opens the file read-only and `--property busy_timeout=<ms>` changes the default five-second wait for a locked file, and each `--command` is one statement. SQLite is dynamically typed: a column reports the type its schema declares, and otherwise the kind of the first value it carries. A `:memory:` database only lives for the current invocation. Official links target the SQLite documentation; the documentation root is https://sqlite.org/docs.html.

## 1. Identify the current connection

**Purpose:** Check the SQLite library version and the attached database file before operating on a target. No placeholders need replacement.

```sql
SELECT sqlite_version() AS library_version,
       (SELECT file FROM pragma_database_list WHERE name = 'main') AS database_file;
```

**Result:** One row. A NULL `database_file` means the connection uses an in-memory database.

**Official documentation:** [sqlite_version](https://sqlite.org/lang_corefunc.html#sqlite_version) · [pragma_database_list](https://sqlite.org/pragma.html#pragma_database_list)

## 2. List tables, views and indexes

**Purpose:** Discover queryable objects. No placeholders need replacement.

```sql
SELECT type, name FROM sqlite_schema WHERE type IN ('table','view','index') ORDER BY type, name;
```

**Result:** One row per object. Internal tables whose names start with `sqlite_` also appear here.

**Official documentation:** [sqlite_schema](https://sqlite.org/schematab.html)

## 3. Inspect a table

**Purpose:** Read the column definitions, types and constraints of one table. Replace `table_name`.

```sql
SELECT name, type, "notnull", dflt_value, pk FROM pragma_table_info('table_name');
```

**Result:** One row per column, in declaration order. A zero `pk` means the column is not part of the primary key.

**Official documentation:** [pragma_table_info](https://sqlite.org/pragma.html#pragma_table_info)

## 4. Read rows with a bound

**Purpose:** Read a page of rows without loading a whole table. Replace `table_name` and `column_name`.

```sql
SELECT column_name FROM table_name ORDER BY column_name LIMIT 100 OFFSET 0;
```

**Result:** Up to 100 rows. Raise `OFFSET` by the page size to continue; `LIMIT -1` means no limit.

**Official documentation:** [SELECT](https://sqlite.org/lang_select.html)

## 5. Write rows

**Purpose:** Insert or update rows, keeping exact numbers. Replace `table_name` and `column_name`.

```sql
INSERT INTO table_name (column_name) VALUES ('value');
UPDATE table_name SET column_name = 'value' WHERE rowid = 1;
DELETE FROM table_name WHERE rowid = 1;
```

**Result:** SQLite reports the affected-row count, which the CLI prints as `affected`. Writing takes a lock on the whole file, so a second writer waits for `busy_timeout` and then fails.

**Official documentation:** [INSERT](https://sqlite.org/lang_insert.html) · [UPDATE](https://sqlite.org/lang_update.html) · [DELETE](https://sqlite.org/lang_delete.html)

## 6. Create or change a table

**Purpose:** Add a table or one column; SQLite has no arbitrary `ALTER TABLE` and no `ALTER COLUMN`. Replace `table_name` and `column_name`.

```sql
CREATE TABLE IF NOT EXISTS table_name (id INTEGER PRIMARY KEY, column_name TEXT);
CREATE INDEX IF NOT EXISTS table_name_column_idx ON table_name (column_name);
ALTER TABLE table_name ADD COLUMN extra TEXT;
```

**Result:** The statements succeed without rows. SQLite rejects adding a column with a non-constant default, and a dropped column stays in the file until `VACUUM`.

**Official documentation:** [CREATE TABLE](https://sqlite.org/lang_createtable.html) · [ALTER TABLE](https://sqlite.org/lang_altertable.html)

## 7. Use SQLite's own features

**Purpose:** Reach a value that SQLite can compute but that has no portable SQL spelling. No placeholders need replacement.

```sql
SELECT json_extract('{"a":[1,2]}', '$.a[1]') AS second,
       random() AS noise,
       strftime('%Y-%m-%dT%H:%M:%SZ', 'now') AS utc_now;
```

**Result:** One row. JSON functions and most date functions require a full-featured build, which the bundled library is.

**Official documentation:** [JSON functions](https://sqlite.org/json1.html) · [Date functions](https://sqlite.org/lang_datefunc.html)

## 8. Check the file

**Purpose:** Read integrity, page and freelist counters before or after bulk work. No placeholders need replacement.

```sql
PRAGMA integrity_check;
PRAGMA page_count;
PRAGMA freelist_count;
```

**Result:** `integrity_check` answers `ok` or reports the damaged pages. The counters are numbers of pages; multiply by `PRAGMA page_size` for bytes.

**Official documentation:** [PRAGMA](https://sqlite.org/pragma.html)

## 9. Reclaim space

**Purpose:** Rebuild the file after large deletes or a dropped column. No placeholders need replacement.

```sql
VACUUM;
ANALYZE;
```

**Result:** `VACUUM` rewrites the file and needs free disk space equal to its size; `ANALYZE` refreshes the planner statistics. Neither returns rows. SQLite accepts exactly one statement per `--command` and rejects an argument that carries several, so repeat `--command` for a batch; the arguments share one connection in order.
