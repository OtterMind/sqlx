# ClickHouse database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. ClickHouse accepts one statement per request, so every operation is its own `--sql` argument.

Replace `database_name` and `table_name` with actual identifiers. Quote identifiers with backticks or double quotes.

ClickHouse is reached through its official JDBC driver, `--type clickhouse`, which connects to the HTTP port (8123 by default) rather than the native 9000 port. Official links target the current ClickHouse documentation. Match the connected server version when you look them up.

## 1. Identify the current connection

**Purpose:** Check the server version, current database, and authenticated user before operating on a target. No placeholders need replacement.

```sql
SELECT version() AS server_version,
       currentDatabase() AS current_database,
       currentUser() AS authenticated_account;
```

**Result:** One context row.

**Official documentation:** [Other functions](https://clickhouse.com/docs/en/sql-reference/functions/other-functions)

## 2. List visible databases

**Purpose:** Discover database names visible to the connected user. No placeholders need replacement.

```sql
SHOW DATABASES;
```

**Result:** One database name per row. Permissions can restrict this list.

**Official documentation:** [SHOW statements](https://clickhouse.com/docs/en/sql-reference/statements/show)

## 3. List tables in a database

**Purpose:** Discover queryable tables. Replace `database_name` with the target database.

```sql
SHOW TABLES FROM database_name;
```

**Result:** One table name per row.

**Official documentation:** [SHOW TABLES](https://clickhouse.com/docs/en/sql-reference/statements/show#show-tables)

## 4. Read a table's CREATE statement

**Purpose:** Obtain server-generated table DDL, including the engine and sorting key. Replace the database and table identifiers.

```sql
SHOW CREATE TABLE database_name.table_name;
```

**Result:** The CREATE TABLE statement. ClickHouse tables always name an engine; `MergeTree` variants carry `ORDER BY`, while `Memory` has none.

**Official documentation:** [SHOW CREATE TABLE](https://clickhouse.com/docs/en/sql-reference/statements/show#show-create-table)

## 5. Inspect columns and types

**Purpose:** Inspect column names, types, defaults, and codecs. Replace the database and table identifiers.

```sql
DESCRIBE TABLE database_name.table_name;
```

**Result:** One row per column with `name`, `type`, `default_type`, `default_expression`, `comment`, `codec_expression`, and `ttl_expression`.

**Official documentation:** [DESCRIBE TABLE](https://clickhouse.com/docs/en/sql-reference/statements/describe-table)

## 6. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the database and table identifiers.

```sql
SELECT * FROM database_name.table_name LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [LIMIT](https://clickhouse.com/docs/en/sql-reference/statements/select/limit)

## 7. Inspect table parts and sizes

**Purpose:** Read storage-level information for a table without scanning its rows. Replace the database and table identifiers.

```sql
SELECT partition, rows, bytes_on_disk
FROM system.parts
WHERE database = 'database_name' AND table = 'table_name' AND active;
```

**Result:** One row per active part, with row counts and on-disk bytes. This reads the `system` database, which is available to every user.

**Official documentation:** [system.parts](https://clickhouse.com/docs/en/operations/system-tables/parts)

ClickHouse has no transactions or rollback: a failed statement leaves earlier statements applied. `INSERT` is the only way to add rows and `ALTER TABLE ... UPDATE`/`DELETE` are asynchronous mutations. The JDBC driver rejects a result set whose columns share a label, so a query returning duplicate aliases fails on ClickHouse.
