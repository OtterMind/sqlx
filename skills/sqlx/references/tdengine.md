# TDengine database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `database_name`, `table_name`, and `stable_name` with actual identifiers. Quote identifiers with backticks and double embedded backticks. Single-quoted SQL strings have separate escaping rules.

`--type tdengine` (alias `taos`) connects through the official JDBC driver in its WebSocket mode, which reaches taosAdapter on port 6041 and needs no native client library. The first column of a table is a `TIMESTAMP` column; TDengine is a time-series database with one table per series or a supertable with subtables. Official links target the TDengine 3.4 SQL manual; match the connected server version when you look them up. The documentation root is https://docs.tdengine.com/3.4.0/tdengine-reference/sql-manual/.

## 1. Identify the current connection

**Purpose:** Check the server version and the current database. No placeholders need replacement.

```sql
SELECT server_version() AS version,
       DATABASE() AS current_database,
       CURRENT_USER() AS authenticated_account;
```

**Result:** One context row. `server_version()` reports the release, for example `3.3.6.13`.

**Official documentation:** [SQL manual](https://docs.tdengine.com/3.4.0/tdengine-reference/sql-manual/)

## 2. List visible databases

**Purpose:** Discover database names. No placeholders need replacement.

```sql
SHOW DATABASES;
```

**Result:** One database name per row, including the system database `information_schema`.

**Official documentation:** [SHOW commands](https://docs.tdengine.com/3.4.0/tdengine-reference/sql-manual/show-commands/)

## 3. List tables in a database

**Purpose:** Discover tables, including the subtables of a supertable. Replace `database_name`.

```sql
SHOW `database_name`.TABLES;
```

**Result:** One row per table with its name, creation time, number of columns, and stable name. `SHOW TABLE DISTRIBUTED database_name.table_name` describes how a table is spread over vgroups.

**Official documentation:** [SHOW commands](https://docs.tdengine.com/3.4.0/tdengine-reference/sql-manual/show-commands/)

## 4. Inspect columns and tags

**Purpose:** Inspect a table's columns, their types, and the tags that identify a subtable. Replace the identifiers.

```sql
DESCRIBE `database_name`.`table_name`;
```

**Result:** One row per column with `field`, `type`, `length`, and the encoding and compression settings; tags follow the columns.

**Official documentation:** [Data types](https://docs.tdengine.com/3.4.0/tdengine-reference/sql-manual/data-types/)

## 5. Read a table's CREATE statement

**Purpose:** Obtain the DDL used to recreate a table or supertable. Replace the identifiers.

```sql
SHOW CREATE TABLE `database_name`.`table_name`;
SHOW CREATE STABLE `database_name`.`stable_name`;
```

**Result:** One row with the object name and its CREATE statement, including the column list, tags, and the database options in effect.

**Official documentation:** [SHOW commands](https://docs.tdengine.com/3.4.0/tdengine-reference/sql-manual/show-commands/)

## 6. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the identifiers.

```sql
SELECT * FROM `database_name`.`table_name` LIMIT 100;
```

**Result:** At most 100 rows. Use `ORDER BY ts DESC` for the newest rows, and always bound time-series queries with a `WHERE ts` range on large tables.

**Official documentation:** [Query data](https://docs.tdengine.com/3.4.0/tdengine-reference/sql-manual/query-data/)

## 7. Read the database's retention and precision settings

**Purpose:** Check how long a database keeps data and how it stores timestamps. Replace `database_name`.

```sql
SELECT name, `keep`, `precision` FROM information_schema.ins_databases WHERE name = 'database_name';
```

**Result:** One row per parameter, including `keep` (retention days), `precision`, and `duration`. Retention matters before promising that old data is still available.

**Official documentation:** [SHOW commands](https://docs.tdengine.com/3.4.0/tdengine-reference/sql-manual/show-commands/)

TDengine stores time series rather than rows in a relational sense: the first column is always a `TIMESTAMP`, a table without a supertable accepts one series, writes are appended and never updated in place, and `DELETE` removes whole time ranges. Several words are reserved in TDengine, including `value`, `keep`, `precision` and `server_version`, so alias result columns with another name or quote them with backticks. Each `--sql` argument is one statement, and an asynchronous write whose outcome is unknown must be verified with a query before it is repeated.
