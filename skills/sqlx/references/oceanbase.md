# OceanBase database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `database_name`, `table_name`, `tenant_name`, and `user_name` with actual identifiers. Quote identifiers with backticks and double embedded backticks. Single-quoted SQL strings have separate escaping rules.

OceanBase serves the MySQL protocol in its MySQL mode, so `--type oceanbase` uses the MySQL worker and a `mysql` datasource reaches the same server. The default port is 2881. A connection usually names a tenant-qualified user such as `root@sys` or `app@tenant`, which is what `--username` carries. Official links target the current OceanBase documentation; match the connected server version when you look them up. The documentation root is https://www.oceanbase.com/docs.

## 1. Identify the current connection

**Purpose:** Check the server version, default database, and authenticated account, including the tenant the account belongs to. No placeholders need replacement.

```sql
SELECT VERSION() AS server_version,
       DATABASE() AS current_database,
       CURRENT_USER() AS authenticated_account;
```

**Result:** One context row. `CURRENT_USER()` reports the tenant-qualified account, for example `root@sys`. A NULL `current_database` means that no default database is selected.

**Official documentation:** [OceanBase documentation](https://www.oceanbase.com/docs)

## 2. List visible databases

**Purpose:** Discover database names visible to the connected account. No placeholders need replacement.

```sql
SHOW DATABASES;
```

**Result:** One database name per row. OceanBase reserves the system databases `oceanbase`, `mysql`, `information_schema`, `performance_schema`, and `sys`.

**Official documentation:** [OceanBase documentation](https://www.oceanbase.com/docs)

## 3. List tables and views in a database

**Purpose:** Discover queryable objects. Replace `database_name` with the target database.

```sql
SHOW FULL TABLES FROM `database_name`;
```

**Result:** Two columns per row: the object name and `BASE TABLE` or `VIEW`.

**Official documentation:** [OceanBase documentation](https://www.oceanbase.com/docs)

## 4. Read a table's CREATE statement

**Purpose:** Obtain the DDL used to recreate the table, including its partitioning and table options. Replace the identifiers.

```sql
SHOW CREATE TABLE `database_name`.`table_name`;
```

**Result:** One row with the table name and its CREATE TABLE statement. OceanBase adds partition and locality clauses that MySQL does not have.

**Official documentation:** [OceanBase documentation](https://www.oceanbase.com/docs)

## 5. Inspect columns and comments

**Purpose:** Inspect a table's column types, nullability, keys, defaults, and comments. Replace the identifiers.

```sql
SHOW FULL COLUMNS FROM `database_name`.`table_name`;
```

**Result:** One row per column with `Field`, `Type`, `Collation`, `Null`, `Key`, `Default`, `Extra`, `Privileges`, and `Comment`.

**Official documentation:** [OceanBase documentation](https://www.oceanbase.com/docs)

## 6. Inspect indexes and column order

**Purpose:** List the indexes of a table and the column order inside each index. Replace the identifiers.

```sql
SHOW INDEX FROM `database_name`.`table_name`;
```

**Result:** One row per index column with `Key_name`, `Seq_in_index`, `Column_name`, `Collation`, `Cardinality`, and `Index_type`.

**Official documentation:** [OceanBase documentation](https://www.oceanbase.com/docs)

## 7. Inspect table size and partitions

**Purpose:** Judge how large a table is and how it is partitioned before scanning it. Replace the identifiers.

```sql
SHOW TABLE STATUS FROM `database_name` LIKE 'table_name';
```

**Result:** One row per matching table with engine, row estimate, data length, index length, and creation time. The row count is an estimate.

**Official documentation:** [OceanBase documentation](https://www.oceanbase.com/docs)

## 8. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the identifiers.

```sql
SELECT * FROM `table_name` LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [OceanBase documentation](https://www.oceanbase.com/docs)

OceanBase runs in MySQL mode by default; the same server can also expose an Oracle-mode tenant, which this recipe does not cover because the CLI connects through the MySQL protocol. Each `--sql` argument is one statement. Distributed transactions and partitions can make a statement slow rather than wrong, so bound exploratory queries and never replay an uncertain write.
