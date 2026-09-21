# StarRocks database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `catalog_name`, `database_name`, `table_name`, and `label_name` with actual identifiers. Quote identifiers with backticks and double embedded backticks. Single-quoted SQL strings have separate escaping rules.

StarRocks speaks the MySQL protocol, so `--type starrocks` uses the MySQL worker and a `mysql` datasource reaches the same server. StarRocks serves the protocol on port 9030 by default and has no user database until one is created. Official links target the StarRocks 4.0 documentation; check the connected server version with `SELECT current_version()` and select the matching documentation version when needed.

## 1. Identify the current frontend and backend

**Purpose:** Check the frontend version and confirm that a backend is registered before creating a table. No placeholders need replacement.

```sql
SELECT current_version() AS frontend_version,
       current_catalog,
       current_database(),
       current_user();
SHOW BACKENDS;
```

**Result:** The first statement returns one context row. `SHOW BACKENDS` returns one row per backend; a table cannot be created until at least one row reports `Alive: true`.

**Official documentation:** [SHOW BACKENDS](https://docs.starrocks.io/docs/4.0/sql-reference/sql-statements/cluster-management/nodes_processes/SHOW_BACKENDS/)

## 2. List catalogs and databases

**Purpose:** Discover the default catalog and the databases inside it. No placeholders need replacement.

```sql
SHOW CATALOGS;
SHOW DATABASES;
```

**Result:** One name per row each. `default_catalog` holds internal tables; external catalogs such as `hive` or `iceberg` appear only when configured.

**Official documentation:** [SHOW DATABASES](https://docs.starrocks.io/docs/4.0/sql-reference/sql-statements/Database/SHOW_DATABASES/)

## 3. List tables in a database

**Purpose:** Discover queryable tables and views. Replace `database_name`.

```sql
SHOW TABLES FROM `database_name`;
```

**Result:** One table name per row.

**Official documentation:** [SHOW TABLES](https://docs.starrocks.io/docs/4.0/sql-reference/sql-statements/table_bucket_part_index/SHOW_TABLES/)

## 4. Inspect columns

**Purpose:** Inspect a table's columns, types, keys, and generated expressions. Replace the identifiers.

```sql
DESCRIBE `database_name`.`table_name`;
SHOW FULL COLUMNS FROM `database_name`.`table_name`;
```

**Result:** One row per column with `Field`, `Type`, `Null`, `Key`, `Default`, and `Extra`. `SHOW FULL COLUMNS` adds collation and comment details.

**Official documentation:** [DESCRIBE](https://docs.starrocks.io/docs/4.0/sql-reference/sql-statements/table_bucket_part_index/DESCRIBE/)

## 5. Read a table's CREATE statement

**Purpose:** Obtain the DDL that StarRocks would use to recreate the table, including its key model and properties. Replace the identifiers.

```sql
SHOW CREATE TABLE `catalog_name`.`database_name`.`table_name`;
```

**Result:** One row with the table name and its CREATE TABLE statement, including `ENGINE`, `DISTRIBUTED BY`, `PARTITION BY`, and `PROPERTIES` such as the replication number.

**Official documentation:** [SHOW CREATE TABLE](https://docs.starrocks.io/docs/4.0/sql-reference/sql-statements/table_bucket_part_index/SHOW_CREATE_TABLE/)

## 6. Inspect partitions and tablets

**Purpose:** See how a table is partitioned and how its data is distributed. Replace the identifiers.

```sql
SHOW PARTITIONS FROM `database_name`.`table_name`;
SHOW TABLET FROM `database_name`.`table_name`;
```

**Result:** One row per partition or tablet with its ID, key range, row count, data size, and replica distribution.

**Official documentation:** [SHOW PARTITIONS](https://docs.starrocks.io/docs/4.0/sql-reference/sql-statements/table_bucket_part_index/SHOW_PARTITIONS/) · [SHOW TABLET](https://docs.starrocks.io/docs/4.0/sql-reference/sql-statements/table_bucket_part_index/SHOW_TABLET/)

## 7. Create a table with an explicit replica count

**Purpose:** Create a table that a single-backend fixture can hold. Replace `database_name`, `table_name`, and the column definitions.

```sql
CREATE TABLE `database_name`.`table_name` (
  `id` BIGINT NOT NULL,
  `label` VARCHAR(100)
) ENGINE = OLAP
DISTRIBUTED BY HASH(`id`) BUCKETS 1
PROPERTIES ("replication_num" = "1");
```

**Result:** StarRocks creates the table. The default replica count is 3, so a cluster with fewer backends rejects a CREATE TABLE without an explicit `replication_num`.

**Official documentation:** [CREATE TABLE](https://docs.starrocks.io/docs/4.0/sql-reference/sql-statements/table_bucket_part_index/CREATE_TABLE/)

## 8. Inspect indexes

**Purpose:** List the indexes of a table before changing or dropping them. Replace the identifiers.

```sql
SHOW INDEX FROM `database_name`.`table_name`;
```

**Result:** One row per index column with its name, column, sequence, and index type.

**Official documentation:** [SHOW INDEX](https://docs.starrocks.io/docs/4.0/sql-reference/sql-statements/table_bucket_part_index/SHOW_INDEX/)

## 9. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the identifiers.

```sql
SELECT * FROM `database_name`.`table_name` LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [SELECT](https://docs.starrocks.io/docs/4.0/sql-reference/sql-statements/table_bucket_part_index/SELECT/)

StarRocks is an OLAP database: `CREATE TABLE` requires a key model, a distribution strategy, and enough live backends for the replication number, and the default catalog decides which external systems a query can reach. A CREATE TABLE that reports "Table replication num should be less than or equal to the number of available backends" means no backend is registered yet, not that the statement is wrong. DDL and DML through the MySQL protocol are atomic per statement, and each `--sql` argument is one statement.
