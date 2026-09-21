# Apache Doris database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `catalog_name`, `database_name`, `table_name`, and `job_name` with actual identifiers. Quote identifiers with backticks and double embedded backticks. Single-quoted SQL strings have separate escaping rules.

Doris speaks the MySQL protocol, so `--type doris` uses the MySQL worker and a `mysql` datasource reaches the same server. Doris serves the protocol on port 9030 by default. Official links target the current Doris documentation; check the connected server version with `SELECT version()` and select the matching documentation version when needed.

## 1. Identify the current frontend and backend

**Purpose:** Check the frontend version and confirm that a backend is registered before creating a table. No placeholders need replacement.

```sql
SELECT version() AS frontend_version,
       current_catalog,
       DATABASE(),
       current_user();
SHOW BACKENDS;
```

**Result:** The first statement returns one context row. `SHOW BACKENDS` returns one row per backend; a table cannot be created until at least one row reports `Alive: true`.

**Official documentation:** [SHOW BACKENDS](https://doris.apache.org/docs/sql-manual/sql-statements/cluster-management/instance-management/SHOW-BACKENDS/)

## 2. List catalogs and databases

**Purpose:** Discover the default catalog and the databases inside it. No placeholders need replacement.

```sql
SHOW CATALOGS;
SHOW DATABASES;
```

**Result:** One name per row each. `internal` holds Doris-managed tables; external catalogs such as `hive`, `iceberg`, or `jdbc` appear only when configured.

**Official documentation:** [SHOW DATABASES](https://doris.apache.org/docs/sql-manual/sql-statements/database/SHOW-DATABASES/)

## 3. List tables in a database

**Purpose:** Discover queryable tables and views. Replace `database_name`.

```sql
SHOW TABLES FROM `database_name`;
```

**Result:** One table name per row.

**Official documentation:** [SHOW TABLES](https://doris.apache.org/docs/sql-manual/sql-statements/table-and-view/table/SHOW-TABLES/)

## 4. Inspect columns

**Purpose:** Inspect a table's columns, types, keys, and aggregation types. Replace the identifiers.

```sql
DESC `database_name`.`table_name` ALL;
```

**Result:** One row per column with `Field`, `Type`, `Null`, `Key`, `Default`, and `Extra`. `DESC ... ALL` also lists hidden columns.

**Official documentation:** [DESC](https://doris.apache.org/docs/sql-manual/sql-statements/table-and-view/table/DESC-TABLE/)

## 5. Read a table's CREATE statement

**Purpose:** Obtain the DDL that Doris would use to recreate the table, including its data model and properties. Replace the identifiers.

```sql
SHOW CREATE TABLE `catalog_name`.`database_name`.`table_name`;
```

**Result:** One row with the table name and its CREATE TABLE statement, including `ENGINE`, `DISTRIBUTED BY`, and `PROPERTIES` such as the replication number.

**Official documentation:** [SHOW CREATE TABLE](https://doris.apache.org/docs/sql-manual/sql-statements/table-and-view/table/SHOW-CREATE-TABLE/)

## 6. Inspect table status and data size

**Purpose:** Judge how a table is stored and how large it is. Replace the identifiers.

```sql
SHOW TABLE STATUS FROM `database_name` LIKE 'table_name';
```

**Result:** One row per matching table with engine, row estimate, data length, index length, and creation time.

**Official documentation:** [SHOW TABLE STATUS](https://doris.apache.org/docs/sql-manual/sql-statements/table-and-view/table/SHOW-TABLE-STATUS/)

## 7. Create a table with an explicit replica count

**Purpose:** Create a table that a single-backend fixture can hold. Replace the identifiers and the column definitions.

```sql
CREATE TABLE `database_name`.`table_name` (
  `id` BIGINT NOT NULL,
  `label` VARCHAR(100)
) ENGINE = OLAP
DUPLICATE KEY(`id`)
DISTRIBUTED BY HASH(`id`) BUCKETS 1
PROPERTIES ("replication_num" = "1");
```

**Result:** Doris creates the table. The default replica count is 3, so a cluster with fewer backends rejects a CREATE TABLE without an explicit `replication_num`, and the error names the replication number and the backends it found.

**Official documentation:** [CREATE TABLE](https://doris.apache.org/docs/sql-manual/sql-statements/table-and-view/table/CREATE-TABLE/)

## 8. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the identifiers.

```sql
SELECT * FROM `database_name`.`table_name` LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [SELECT](https://doris.apache.org/docs/sql-manual/sql-statements/table-and-view/table/SELECT/)

Doris is an OLAP database: every CREATE TABLE declares a data model (`DUPLICATE`, `AGGREGATE`, or `UNIQUE`), a distribution strategy, and enough live backends for the replication number, while the selected catalog decides which external systems a query can reach. SQL written for MySQL mostly runs unchanged, but the documented differences apply. Each `--sql` argument is one statement, so keep multi-statement maintenance jobs in separate invocations and never replay an uncertain write.
