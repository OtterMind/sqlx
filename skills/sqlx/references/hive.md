# Hive operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `database_name`, `table_name`, and `column_name` with actual identifiers. Hive is case-insensitive for unquoted identifiers and stores them in lower case, so `Orders` and `orders` reach the same table; use backticks when an identifier needs exact case, spaces, or a keyword. String literals use single quotes.

`--type hive` runs on the JDBC worker with the driver `org.apache.hive.jdbc.HiveDriver`, and the SQLX component ships the standalone Hive 4.0.1 driver. The driver builds `jdbc:hive2://host:port/database`, so `--database` names the Hive database, often `default`, and the default port is 10000. Users are not authenticated by default: `--username` is whatever the server was configured to expect, often `hive`. Official links target the Apache Hive language manual; the documentation root is https://hive.apache.org/.

## 1. Identify the current connection

**Purpose:** Check the Hive version, the current database, and the session user before operating on a target. No placeholders need replacement.

```sql
SELECT version() AS engine_version,
       current_database() AS database_name,
       current_user() AS session_user;
```

**Result:** One context row. `current_user()` reports the user name HiveServer2 received, which on an unauthenticated server is whatever `--username` sent.

**Official documentation:** [Operators and UDFs](https://hive.apache.org/docs/latest/language/languagemanual-udf/) · [Commands](https://hive.apache.org/docs/latest/language/languagemanual-commands/)

## 2. List databases

**Purpose:** Discover the databases the metastore exposes. No placeholders need replacement.

```sql
SHOW DATABASES;
```

**Result:** One database name per row. Metadata is served by the Hive metastore, so this statement is cheap and does not start a cluster job.

**Official documentation:** [DDL statements](https://hive.apache.org/docs/latest/language/languagemanual-ddl/)

## 3. List tables and views

**Purpose:** Discover queryable objects in one database. Replace `database_name`.

```sql
SHOW TABLES IN database_name;
```

**Result:** One table or view name per row. Table metadata can also be read through `information_schema`, but not every Hive version enables it.

**Official documentation:** [DDL statements](https://hive.apache.org/docs/latest/language/languagemanual-ddl/)

## 4. Inspect columns and storage

**Purpose:** Read column names and types, then the location, input format, and SerDe behind them. Replace `database_name` and `table_name`.

```sql
DESCRIBE database_name.table_name;
DESCRIBE FORMATTED database_name.table_name;
```

**Result:** The first statement returns one row per column in declaration order, including partition columns at the end. The second adds `Location`, `InputFormat`, `SerDe`, and table properties, which explain why a table can be empty although its files exist.

**Official documentation:** [DDL statements](https://hive.apache.org/docs/latest/language/languagemanual-ddl/)

## 5. Read a table's definition

**Purpose:** Obtain the CREATE TABLE statement that reproduces a table. Replace `database_name` and `table_name`.

```sql
SHOW CREATE TABLE database_name.table_name;
```

**Result:** The CREATE TABLE statement, including the partition clauses and table properties. Hive has no `SHOW CREATE VIEW`; for a view, read its definition from the metastore instead.

**Official documentation:** [DDL statements](https://hive.apache.org/docs/latest/language/languagemanual-ddl/)

## 6. Inspect and repair partitions

**Purpose:** See which partitions the metastore knows, and register files that were added outside Hive. Replace `database_name` and `table_name`.

```sql
SHOW PARTITIONS database_name.table_name;
MSCK REPAIR TABLE database_name.table_name SYNC PARTITIONS;
```

**Result:** One partition per row for `SHOW PARTITIONS`. `MSCK REPAIR TABLE ... SYNC PARTITIONS` registers or drops partitions so the metastore matches the file system; Hive 4 uses the `SYNC PARTITIONS` spelling, and older versions accept `MSCK REPAIR TABLE table_name`. Neither statement moves data, and a repair over many partitions is slow.

**Official documentation:** [DDL statements](https://hive.apache.org/docs/latest/language/languagemanual-ddl/)

## 7. Bound every large read

**Purpose:** Read a page of rows without scanning the whole table. Replace `database_name`, `table_name`, and `column_name`.

```sql
SELECT column_name FROM database_name.table_name LIMIT 100;
SELECT count(*) AS row_count FROM database_name.table_name;
```

**Result:** Up to 100 rows for the first statement, one count row for the second. Hive has no safe unbounded scan: every `SELECT` compiles to a job on the cluster and reads the table's files, so always add `LIMIT` or restrict with a partition predicate.

**Official documentation:** [Select](https://hive.apache.org/docs/latest/language/languagemanual-select/) · [DML](https://hive.apache.org/docs/latest/language/languagemanual-dml/)
