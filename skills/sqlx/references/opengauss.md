# openGauss database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `schema_name`, `table_name`, and `column_name` with actual identifiers. Quote identifiers with double quotes when they need case or special characters; unquoted names are folded to lower case.

openGauss speaks the PostgreSQL protocol, so `--type opengauss` (alias `gaussdb`) uses the PostgreSQL worker and a `postgresql` datasource reaches the same server. It serves the protocol on port 5432 by default. Official links target the current openGauss documentation; match the connected server version when you look them up. The documentation root is https://docs.opengauss.org/en/.

## 1. Identify the current connection

**Purpose:** Check the server version, current database, schema, and user before operating on a target. No placeholders need replacement.

```sql
SELECT version() AS server_version,
       current_database(),
       current_schema(),
       current_user;
```

**Result:** One context row. `server_version` reports the openGauss release, for example `(openGauss 7.0.0-RC3 build ...)`.

**Official documentation:** [openGauss SQL reference](https://docs.opengauss.org/en/docs/latest/docs/SQLReference/SQLReference.html)

## 2. List databases and schemas

**Purpose:** Discover databases and the schemas inside the current database. No placeholders need replacement.

```sql
SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY datname;
SELECT schema_name FROM information_schema.schemata ORDER BY schema_name;
```

**Result:** One name per row. `pg_database` lists databases; `information_schema.schemata` lists schemas of the connected database.

**Official documentation:** [openGauss SQL reference](https://docs.opengauss.org/en/docs/latest/docs/SQLReference/SQLReference.html)

## 3. List tables in a schema

**Purpose:** Discover queryable tables and views. Replace `schema_name` with the target schema.

```sql
SELECT table_name, table_type
FROM information_schema.tables
WHERE table_schema = 'schema_name'
ORDER BY table_name;
```

**Result:** One row per object with `BASE TABLE` or `VIEW` as its type.

**Official documentation:** [openGauss SQL reference](https://docs.opengauss.org/en/docs/latest/docs/SQLReference/SQLReference.html)

## 4. Inspect columns

**Purpose:** Inspect column names, types, nullability, and defaults. Replace the identifiers.

```sql
SELECT column_name, data_type, is_nullable, column_default
FROM information_schema.columns
WHERE table_schema = 'schema_name' AND table_name = 'table_name'
ORDER BY ordinal_position;
```

**Result:** One row per column in table order.

**Official documentation:** [openGauss SQL reference](https://docs.opengauss.org/en/docs/latest/docs/SQLReference/SQLReference.html)

## 5. Inspect indexes and constraints

**Purpose:** List a table's indexes and its constraints. Replace the identifiers.

```sql
SELECT indexname, indexdef
FROM pg_indexes
WHERE schemaname = 'schema_name' AND tablename = 'table_name'
ORDER BY indexname;
```

**Result:** One row per index, including primary key and unique indexes, with the full CREATE INDEX definition.

**Official documentation:** [openGauss SQL reference](https://docs.opengauss.org/en/docs/latest/docs/SQLReference/SQLReference.html)

## 6. Read a column's comment and the table's columns

**Purpose:** Inspect column comments, which openGauss stores in the system catalog rather than in `information_schema`. Replace the identifiers.

```sql
SELECT a.attname AS column_name,
       col_description(a.attrelid, a.attnum) AS column_comment
FROM pg_attribute AS a
JOIN pg_class AS c ON c.oid = a.attrelid
JOIN pg_namespace AS n ON n.oid = c.relnamespace
WHERE n.nspname = 'schema_name' AND c.relname = 'table_name'
  AND a.attnum > 0 AND NOT a.attisdropped
ORDER BY a.attnum;
```

**Result:** One row per column with its comment, or NULL when no comment was set.

**Official documentation:** [openGauss SQL reference](https://docs.opengauss.org/en/docs/latest/docs/SQLReference/SQLReference.html)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the identifiers.

```sql
SELECT * FROM "schema_name"."table_name" LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [openGauss SQL reference](https://docs.opengauss.org/en/docs/latest/docs/SQLReference/SQLReference.html)

openGauss keeps the PostgreSQL catalog and information schema, so unquoted identifiers fold to lower case and `SHOW CREATE TABLE` does not exist: read the definition from `pg_indexes`, `information_schema`, and the catalog instead. Each `--sql` argument is one statement, and a statement whose outcome is unknown must not be replayed automatically.
