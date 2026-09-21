# CockroachDB database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `database_name`, `table_name`, and `view_name` with actual identifiers. Quote identifiers with double quotes and double embedded quotes; unquoted identifiers fold to lowercase, as in PostgreSQL.

CockroachDB speaks the PostgreSQL wire protocol, so `--type cockroachdb` uses the PostgreSQL worker and a `postgresql` datasource reaches the same server. Default connections use port 26257. Official links target the stable CockroachDB documentation. Match the connected server version when you look them up.

## 1. Identify the current connection

**Purpose:** Check the server version, current database, and authenticated account before operating on a target. No placeholders need replacement.

```sql
SELECT version() AS server_version,
       current_database() AS current_database,
       current_user AS authenticated_account;
```

**Result:** One context row. A NULL `current_database` means that no default database is selected.

**Official documentation:** [Selection queries](https://www.cockroachlabs.com/docs/stable/selection-queries)

## 2. List visible databases

**Purpose:** Discover database names visible to the connected account. No placeholders need replacement.

```sql
SHOW DATABASES;
```

**Result:** One database name per row. Permissions can restrict this list.

**Official documentation:** [SHOW DATABASES](https://www.cockroachlabs.com/docs/stable/show-databases)

## 3. List tables and views in a database

**Purpose:** Discover queryable objects. Replace `database_name` with the target database.

```sql
SHOW TABLES FROM database_name;
```

**Result:** Table and view names for the selected database.

**Official documentation:** [SHOW TABLES](https://www.cockroachlabs.com/docs/stable/show-tables)

## 4. Read an object's CREATE statement

**Purpose:** Obtain server-generated DDL for a table or view. Replace the database and object identifiers.

```sql
SHOW CREATE TABLE database_name.table_name;
SHOW CREATE VIEW database_name.view_name;
```

**Result:** The CREATE statement for that object, including columns, keys, and indexes for tables. Data and dependent objects require separate handling.

**Official documentation:** [SHOW CREATE](https://www.cockroachlabs.com/docs/stable/show-create)

## 5. Inspect columns

**Purpose:** Inspect types, nullability, defaults, and generated expressions. Replace the database and table identifiers.

```sql
SHOW COLUMNS FROM database_name.table_name;
```

**Result:** One row per column with `column_name`, `data_type`, `is_nullable`, `column_default`, and generation information.

**Official documentation:** [SHOW COLUMNS](https://www.cockroachlabs.com/docs/stable/show-columns)

## 6. Inspect indexes

**Purpose:** Identify indexes, uniqueness, and indexed columns. Replace the database and table identifiers.

```sql
SHOW INDEX FROM database_name.table_name;
```

**Result:** One row per indexed column, including the index name, whether it is unique, and the column order.

**Official documentation:** [SHOW INDEX](https://www.cockroachlabs.com/docs/stable/show-index) · [Indexes](https://www.cockroachlabs.com/docs/stable/indexes)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the database and table identifiers.

```sql
SELECT * FROM database_name.table_name LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real primary key when a repeatable sample matters.

**Official documentation:** [SELECT clause](https://www.cockroachlabs.com/docs/stable/select-clause)

CockroachDB has no `USE` statement; qualify object names or set the datasource database instead. Schema changes run online and are not wrapped in the surrounding transaction. Stored procedures are not supported; use functions or client-side batches.
