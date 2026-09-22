# DuckDB operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `table_name`, `file`, and `column_name` with actual identifiers. DuckDB folds unquoted identifiers to lower case and accepts double-quoted identifiers.

`--type duckdb` opens a local database file: pass it as `--database <file>` or `--path <file>`, and the file is created when it does not exist. `--property read_only=true` opens it read-only, `--property threads=<n>` limits the thread count, and `:memory:` keeps a database for the current invocation only. Writes answer with a result set named `Count` that carries the affected-row count. DuckDB types are exact: `HUGEINT` and `DECIMAL` stay text, `LIST`, `STRUCT`, `MAP` and `ARRAY` arrive as JSON text, `BLOB` as Base64, and timestamps as ISO text. Official links target the DuckDB documentation; the documentation root is https://duckdb.org/docs/stable/.

## 1. Identify the current connection

**Purpose:** Check the version and the attached database file before operating on a target. No placeholders need replacement.

```sql
SELECT version() AS engine_version, current_database() AS current_database, current_schema() AS current_schema;
```

**Result:** One row. The database name is the file path, or `memory` for an in-memory database.

**Official documentation:** [version](https://duckdb.org/docs/stable/sql/functions/utility.html#version) · [current_database](https://duckdb.org/docs/stable/sql/functions/utility.html#current_database)

## 2. List tables and views

**Purpose:** Discover queryable objects in a schema. Replace `schema_name` with the schema to inspect.

```sql
SELECT table_schema, table_name, table_type FROM information_schema.tables
WHERE table_schema = 'schema_name' ORDER BY table_name;
```

**Result:** One row per table or view. `main` is the default schema.

**Official documentation:** [information_schema](https://duckdb.org/docs/stable/sql/meta/information_schema.html)

## 3. Inspect a table

**Purpose:** Read column names, types and nullability of one table. Replace `table_name`.

```sql
SELECT column_name, data_type, is_nullable FROM information_schema.columns
WHERE table_name = 'table_name' ORDER BY ordinal_position;
```

**Result:** One row per column, in declaration order.

**Official documentation:** [information_schema.columns](https://duckdb.org/docs/stable/sql/meta/information_schema.html)

## 4. Query files without importing them

**Purpose:** Read Parquet, CSV or JSON directly, which is why a local analytical query rarely needs a load step. Replace `file`.

```sql
SELECT * FROM 'file' LIMIT 100;
SELECT * FROM read_parquet('file') LIMIT 100;
SELECT * FROM read_csv_auto('file') LIMIT 100;
```

**Result:** Rows straight from the file. The path is a literal, so a shell-quoted argument is the easiest spelling; `read_parquet` also accepts a list of paths.

**Official documentation:** [Reading files](https://duckdb.org/docs/stable/data/overview.html) · [read_parquet](https://duckdb.org/docs/stable/data/parquet/overview.html)

## 5. Write rows

**Purpose:** Insert, update or delete rows and see how many the statement changed. Replace `table_name`.

```sql
INSERT INTO table_name VALUES (1, 'value');
UPDATE table_name SET column_name = 'value' WHERE id = 1;
DELETE FROM table_name WHERE id = 1;
```

**Result:** A one-row `Count` result set with the affected-row count. The `--command` batch stops at the first error, and a failed statement does not roll back earlier ones.

**Official documentation:** [INSERT](https://duckdb.org/docs/stable/sql/statements/insert.html) · [UPDATE](https://duckdb.org/docs/stable/sql/statements/update.html) · [DELETE](https://duckdb.org/docs/stable/sql/statements/delete.html)

## 6. Create or change a table

**Purpose:** Add tables and columns, and change a column type, which DuckDB supports directly. Replace `table_name` and `column_name`.

```sql
CREATE TABLE IF NOT EXISTS table_name (id BIGINT PRIMARY KEY, column_name VARCHAR);
ALTER TABLE table_name ADD COLUMN extra DECIMAL(18,3);
ALTER TABLE table_name ALTER column_name TYPE VARCHAR;
```

**Result:** The statements succeed with a `Count` result set. DuckDB stores a database in one file, so a type change rewrites the column rather than the file.

**Official documentation:** [CREATE TABLE](https://duckdb.org/docs/stable/sql/statements/create_table.html) · [ALTER TABLE](https://duckdb.org/docs/stable/sql/statements/alter_table.html)

## 7. Aggregate and nest

**Purpose:** Summarize rows and build nested values that arrive as JSON text. Replace `table_name`.

```sql
SELECT category, count(*) AS rows_in_group, sum(amount) AS total FROM table_name GROUP BY category ORDER BY total DESC;
SELECT {'id': id, 'tags': [category]} AS record FROM table_name LIMIT 10;
```

**Result:** One row per group, or one JSON object per row. Aggregates over `DECIMAL` keep their scale, and `sum` over `HUGEINT` stays exact.

**Official documentation:** [Aggregate functions](https://duckdb.org/docs/stable/sql/functions/aggregates.html) · [Nested types](https://duckdb.org/docs/stable/sql/data_types/nested.html)

## 8. Check storage

**Purpose:** Read the file's size and the compression of one column before a large import or export. Replace `table_name`.

```sql
PRAGMA database_size;
SELECT * FROM pragma_storage_info('table_name') LIMIT 20;
```

**Result:** `database_size` gives the file size in bytes and blocks; `pragma_storage_info` gives one row per column segment with its compression.

**Official documentation:** [PRAGMA database_size](https://duckdb.org/docs/stable/sql/pragmas.html) · [Storage](https://duckdb.org/docs/stable/internals/storage.html)

## 9. Export results, one statement per `--command`

**Purpose:** Write a query result to a file for another tool. Replace `table_name` and `file`.

```sql
COPY (SELECT * FROM table_name) TO 'file' (FORMAT PARQUET);
COPY table_name TO 'file' (FORMAT CSV, HEADER);
```

**Result:** The statement writes the file and answers with a one-row `Count`. The path is relative to the worker's working directory unless it is absolute. SQLX rejects a `--command` that carries more than one statement, because DuckDB runs them all but reports only the first result; repeat `--command` instead, and the arguments share one connection in order.

**Official documentation:** [COPY](https://duckdb.org/docs/stable/sql/statements/copy.html)
