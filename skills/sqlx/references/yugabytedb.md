# YugabyteDB database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `database_name`, `schema_name`, `table_name`, and `index_name` with actual identifiers. Quote identifiers with double quotes when they need case or special characters; unquoted names are folded to lower case.

YugabyteDB's YSQL API speaks the PostgreSQL protocol, so `--type yugabytedb` uses the PostgreSQL worker and a `postgresql` datasource reaches the same server. YSQL listens on port 5433 by default, and the default superuser is `yugabyte` with the database of the same name. Official links target the YugabyteDB documentation for the current preview release; match the connected server version when you look them up.

## 1. Identify the current connection

**Purpose:** Check the server version, current database, schema, and user before operating on a target. No placeholders need replacement.

```sql
SELECT version() AS server_version,
       current_database(),
       current_schema(),
       current_user;
```

**Result:** One context row. `server_version` reports both the PostgreSQL compatibility version and the YugabyteDB release, for example `PostgreSQL 15.12-YB-2026.1.1.2`.

**Official documentation:** [YSQL statements](https://docs.yugabyte.com/preview/api/ysql/the-sql-language/statements/)

## 2. List databases and schemas

**Purpose:** Discover databases and the schemas inside the current database. No placeholders need replacement.

```sql
SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY datname;
SELECT schema_name FROM information_schema.schemata ORDER BY schema_name;
```

**Result:** One name per row. `pg_database` lists databases; `information_schema.schemata` lists schemas of the connected database.

**Official documentation:** [CREATE DATABASE](https://docs.yugabyte.com/preview/api/ysql/the-sql-language/statements/ddl_create_database/)

## 3. List tables in a schema

**Purpose:** Discover queryable tables and views. Replace `schema_name` with the target schema.

```sql
SELECT table_name, table_type
FROM information_schema.tables
WHERE table_schema = 'schema_name'
ORDER BY table_name;
```

**Result:** One row per object with `BASE TABLE` or `VIEW` as its type.

**Official documentation:** [CREATE TABLE](https://docs.yugabyte.com/preview/api/ysql/the-sql-language/statements/ddl_create_table/)

## 4. Inspect columns

**Purpose:** Inspect column names, types, nullability, and defaults. Replace the identifiers.

```sql
SELECT column_name, data_type, is_nullable, column_default
FROM information_schema.columns
WHERE table_schema = 'schema_name' AND table_name = 'table_name'
ORDER BY ordinal_position;
```

**Result:** One row per column in table order. A non-null `column_default` shows what a stored default resolves to.

**Official documentation:** [Information schema](https://docs.yugabyte.com/preview/api/ysql/the-sql-language/statements/)

## 5. Inspect indexes and constraints

**Purpose:** List a table's indexes with their definitions. Replace the identifiers.

```sql
SELECT indexname, indexdef
FROM pg_indexes
WHERE schemaname = 'schema_name' AND tablename = 'table_name'
ORDER BY indexname;
```

**Result:** One row per index, including primary key and unique indexes, with the full CREATE INDEX definition.

**Official documentation:** [CREATE INDEX](https://docs.yugabyte.com/preview/api/ysql/the-sql-language/statements/ddl_create_index/)

## 6. List the cluster's nodes

**Purpose:** Check which nodes serve the cluster and their role, before judging where data can be placed. No placeholders need replacement.

```sql
SELECT host, port, node_type, cloud, region, zone, rack
FROM yb_servers()
ORDER BY host;
```

**Result:** One row per node with its YSQL port and its `primary`, `read_replica`, or other node type.

**Official documentation:** [yb_servers](https://docs.yugabyte.com/preview/api/ysql/exprs/func_yb_servers/)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the identifiers.

```sql
SELECT * FROM "schema_name"."table_name" LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [SELECT](https://docs.yugabyte.com/preview/api/ysql/the-sql-language/statements/dml_select/)

## 8. Inspect active statements and sessions

**Purpose:** See which statements are running and which sessions hold them. No placeholders need replacement.

```sql
SELECT pid, datname, usename, state, query
FROM pg_stat_activity
WHERE state <> 'idle'
ORDER BY query_start;
```

**Result:** One row per active session. YugabyteDB adds its own columns to this view in addition to the PostgreSQL ones.

**Official documentation:** [SHOW statements](https://docs.yugabyte.com/preview/api/ysql/the-sql-language/statements/cmd_show/)

YugabyteDB runs PostgreSQL-compatible SQL on a distributed, replicated store. Unquoted identifiers are folded to lower case, so alias a column with double quotes when its exact spelling matters. Every `--sql` argument is one statement; `CREATE DATABASE` cannot run inside a transaction block, and a statement whose outcome is unknown must not be replayed automatically.
