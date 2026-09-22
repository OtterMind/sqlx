# KingbaseES database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `schema_name`, `table_name`, and `database_name` with actual identifiers. Quote identifiers with double quotes when they need case or special characters; unquoted names are folded to lower case.

`--type kingbase` (alias `kingbasees`) connects through the vendor JDBC driver, `com.kingbase8.Driver`, on port 54321. KingbaseES is PostgreSQL-compatible, so the PostgreSQL catalog and information schema answer metadata questions. Official links target the Kingbase documentation portal; match the connected server version when you look them up. The documentation root is https://www.kingbase.com.cn/ and its documentation portal is https://help.kingbase.com.cn/.

## 1. Identify the current connection

**Purpose:** Check the server version, current database, and account before operating on a target. No placeholders need replacement.

```sql
SELECT version() AS server_version,
       current_database(),
       current_user;
```

**Result:** One context row. `version()` reports the release, for example `KingbaseES V009R003C018`.

**Official documentation:** [Kingbase documentation portal](https://help.kingbase.com.cn/)

## 2. List databases and schemas

**Purpose:** Discover databases and the schemas inside the current database. No placeholders need replacement.

```sql
SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY datname;
SELECT schema_name FROM information_schema.schemata ORDER BY schema_name;
```

**Result:** One name per row. KingbaseES ships several schemas, including `sys`, `sysaudit`, and `public`.

**Official documentation:** [Kingbase documentation portal](https://help.kingbase.com.cn/)

## 3. List tables in a schema

**Purpose:** Discover queryable tables and views. Replace `schema_name` with the target schema.

```sql
SELECT table_schema, table_name, table_type
FROM information_schema.tables
WHERE table_schema = 'schema_name'
ORDER BY table_name;
```

**Result:** One row per object with `BASE TABLE` or `VIEW` as its type.

**Official documentation:** [Kingbase documentation portal](https://help.kingbase.com.cn/)

## 4. Inspect columns

**Purpose:** Inspect column names, types, nullability, and defaults. Replace the identifiers.

```sql
SELECT ordinal_position, column_name, data_type, is_nullable, column_default
FROM information_schema.columns
WHERE table_schema = 'schema_name' AND table_name = 'table_name'
ORDER BY ordinal_position;
```

**Result:** One row per column in table order. `is_nullable` is `YES` or `NO`.

**Official documentation:** [Kingbase documentation portal](https://help.kingbase.com.cn/)

## 5. Inspect indexes and constraints

**Purpose:** List a table's indexes with their definitions. Replace the identifiers.

```sql
SELECT indexname, indexdef
FROM pg_indexes
WHERE schemaname = 'schema_name' AND tablename = 'table_name'
ORDER BY indexname;
```

**Result:** One row per index, including primary key and unique indexes, with the full CREATE INDEX definition.

**Official documentation:** [Kingbase documentation portal](https://help.kingbase.com.cn/)

## 6. Inspect table size and comments

**Purpose:** Judge how large a table is and read its comment. Replace the identifiers.

```sql
SELECT table_name,
       pg_size_pretty(pg_total_relation_size(quote_ident(table_name))) AS total_size
FROM information_schema.tables
WHERE table_schema = 'schema_name';

SELECT c.relname AS table_name, obj_description(c.oid) AS comment
FROM pg_class AS c
JOIN pg_namespace AS n ON n.oid = c.relnamespace
WHERE n.nspname = 'schema_name' AND c.relname = 'table_name';
```

**Result:** The first statement returns one row per table with a human-readable size; the second returns the table comment, or NULL when none was set.

**Official documentation:** [Kingbase documentation portal](https://help.kingbase.com.cn/)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the identifiers.

```sql
SELECT * FROM "schema_name"."table_name" LIMIT 100;
```

**Result:** At most 100 rows. Without `ORDER BY` the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [Kingbase documentation portal](https://help.kingbase.com.cn/)

## 8. Explain a query plan

**Purpose:** Confirm which plan the server chooses before running a heavy statement. Replace the statement.

```sql
EXPLAIN SELECT * FROM "schema_name"."table_name" WHERE id = 1;
```

**Result:** One row per plan line under the `QUERY PLAN` column. `EXPLAIN ANALYZE` additionally runs the statement and reports actual times, so it executes every side effect.

**Official documentation:** [Kingbase documentation portal](https://help.kingbase.com.cn/)

KingbaseES keeps the PostgreSQL catalog and information schema, so unquoted identifiers fold to lower case and there is no `SHOW CREATE TABLE`; read definitions from `pg_indexes`, `information_schema` and the catalog instead. Administrative schemas such as `sys` and `sysaudit` hold system objects. Each `--command` argument is one statement, and a statement whose outcome is unknown must not be replayed automatically.
