# TiDB database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `database_name`, `table_name`, `index_name`, and `column_name` with actual identifiers. Quote identifiers with backticks and double embedded backticks. Single-quoted SQL strings have separate escaping rules.

TiDB speaks the MySQL protocol, so `--type tidb` uses the MySQL worker and a `mysql` datasource reaches the same server. TiDB defaults to port 4000 and, in the container fixture, the `root` account has no password. Official links target the current TiDB documentation for the stable release. The documentation root is https://docs.pingcap.com/tidb/stable/. Match the connected server version when you look them up.

## 1. Identify the current connection

**Purpose:** Check the server version, default database, and authenticated account before operating on a target. No placeholders need replacement.

```sql
SELECT VERSION() AS server_version,
       DATABASE() AS current_database,
       CURRENT_USER() AS authenticated_account;
```

**Result:** One context row. TiDB reports its MySQL compatibility version and the TiDB release in `VERSION()`, for example `8.0.11-TiDB-v8.5.8`. A NULL `current_database` means that no default database is selected.

**Official documentation:** [TiDB version and MySQL compatibility](https://docs.pingcap.com/tidb/stable/mysql-compatibility/)

## 2. List visible databases

**Purpose:** Discover database names visible to the connected account. No placeholders need replacement.

```sql
SHOW DATABASES;
```

**Result:** One database name per row. Permissions can restrict this list.

**Official documentation:** [SHOW DATABASES](https://docs.pingcap.com/tidb/stable/sql-statement-show-databases/)

## 3. List tables and views in a database

**Purpose:** Discover queryable objects. Replace `database_name` with the target database.

```sql
SHOW FULL TABLES FROM `database_name`;
```

**Result:** Two columns per row: the object name and `BASE TABLE` or `VIEW`. `SHOW TABLES FROM database_name` returns names only.

**Official documentation:** [SHOW TABLES](https://docs.pingcap.com/tidb/stable/sql-statement-show-tables/)

## 4. Inspect columns and indexes

**Purpose:** Inspect a table's column types and its indexes. Replace the identifiers.

```sql
SHOW COLUMNS FROM `table_name` FROM `database_name`;
SHOW INDEX FROM `table_name` FROM `database_name`;
```

**Result:** `SHOW COLUMNS` returns one row per column with its type, nullability, key membership, default, and extra attributes. `SHOW INDEX` returns one row per index column.

**Official documentation:** [SHOW INDEX](https://docs.pingcap.com/tidb/stable/sql-statement-show-index/)

## 5. Read a table's CREATE statement

**Purpose:** Obtain the exact DDL that TiDB would use to recreate the table. Replace the identifiers.

```sql
SHOW CREATE TABLE `database_name`.`table_name`;
```

**Result:** One row with the table name and its CREATE TABLE statement, including TiDB-specific table options such as `AUTO_ID_CACHE` and `SHARD_ROW_ID_BITS`.

**Official documentation:** [SHOW CREATE TABLE](https://docs.pingcap.com/tidb/stable/sql-statement-show-create-table/)

## 6. Inspect table size and row estimates

**Purpose:** Judge how large a table is before scanning it. Replace the identifiers.

```sql
SHOW TABLE STATUS FROM `database_name` LIKE 'table_name';
```

**Result:** One row per matching table with engine, row estimate, average row length, data length, index length, and creation time. The row count is an estimate, not an exact count.

**Official documentation:** [SHOW TABLE STATUS](https://docs.pingcap.com/tidb/stable/sql-statement-show-table-status/)

## 7. Inspect statistics and storage regions

**Purpose:** Check whether TiDB has statistics for a table and how its data is split across regions. Replace the identifiers.

```sql
SHOW STATS_META WHERE db_name = 'database_name' AND table_name = 'table_name';
SHOW TABLE `database_name`.`table_name` REGIONS;
```

**Result:** `SHOW STATS_META` returns one row with row count, modification count, and last update time; a missing row means no statistics were collected. The region statement returns one row per region with its ID, start and end keys, leader store, and replica stores.

**Official documentation:** [SHOW STATS_META](https://docs.pingcap.com/tidb/stable/sql-statement-show-stats-meta/) · [SHOW TABLE REGIONS](https://docs.pingcap.com/tidb/stable/sql-statement-show-table-regions/)

## 8. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the identifiers.

```sql
SELECT * FROM `table_name` LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [SELECT](https://docs.pingcap.com/tidb/stable/sql-statement-select/)

## 9. Explain a query plan

**Purpose:** Confirm that a query uses the expected index or coprocessor pushdown before running it on a large table. Replace the statement.

```sql
EXPLAIN SELECT * FROM `database_name`.`table_name` WHERE `column_name` = 'value';
EXPLAIN ANALYZE SELECT * FROM `database_name`.`table_name` WHERE `column_name` = 'value';
```

**Result:** `EXPLAIN` returns the plan without running it. `EXPLAIN ANALYZE` runs the statement and adds actual row counts and execution time, so it executes every side effect of the statement.

**Official documentation:** [EXPLAIN](https://docs.pingcap.com/tidb/stable/sql-statement-explain/) · [EXPLAIN ANALYZE](https://docs.pingcap.com/tidb/stable/sql-statement-explain-analyze/)

## 10. Read server settings

**Purpose:** Check an optimizer or transaction setting that changes behavior. Replace the variable name or use `LIKE`.

```sql
SHOW VARIABLES LIKE 'tidb_mem_quota_query';
```

**Result:** One row per matching variable with its current session value.

**Official documentation:** [SHOW VARIABLES](https://docs.pingcap.com/tidb/stable/sql-statement-show-variables/)

TiDB accepts MySQL syntax with documented differences: some MySQL statements, functions, and features are unsupported or behave differently, and the differences are listed per release. `information_schema` is available for catalog queries. Each `--sql` argument is one statement, TiDB defaults to optimistic transactions, and an explicit `BEGIN` combined with an error outcome must not be replayed automatically.
