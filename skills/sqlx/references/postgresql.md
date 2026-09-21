# PostgreSQL database operations

Each SQL block is a separate operation submitted through `sqlx sql execute --datasource <id> --sql "..."`. Put statements requiring shared session state in the same invocation.

Choose the datasource's database first, then a schema within it. `public` is an example schema; replace it and all object-name placeholders. Double quotes delimit identifiers, with embedded quotes doubled. Single quotes delimit filter strings. Preserve the exact spelling of quoted mixed-case names.

Official links target PostgreSQL 17. Open the page for the version of the connected server, and when a page has moved, search that vendor's site rather than a third-party copy. With web tools, check the operation's link before relying on it; without them, use this recipe and state that the official page was not checked.

## 1. Identify the current server, database, and schema

**Purpose:** Confirm the connection target and execution identity. No placeholders need replacement.

```sql
SELECT version() AS server_version,
       current_database() AS database_name,
       current_schema() AS schema_name,
       current_user AS account_name;
```

**Result:** One context row. `current_schema()` is the effective default from the search path, not the only schema accessible to the account.

**Official documentation:** [System information functions](https://www.postgresql.org/docs/17/functions-info.html)

## 2. List databases accepting connections

**Purpose:** Discover database names in the cluster. No placeholders need replacement.

```sql
SELECT datname AS database_name
FROM pg_database
WHERE datallowconn
ORDER BY datname;
```

**Result:** One database per row. Listing a database does not prove the account has CONNECT permission. Changing database requires a different connection; psql's `\connect` is not SQL.

**Official documentation:** [pg_database](https://www.postgresql.org/docs/17/catalog-pg-database.html)

## 3. List visible schemas

**Purpose:** Choose a namespace for subsequent object inspection. No placeholders need replacement.

```sql
SELECT schema_name
FROM information_schema.schemata
ORDER BY schema_name;
```

**Result:** Schemas visible to the current account, subject to privileges.

**Official documentation:** [information_schema.schemata](https://www.postgresql.org/docs/17/infoschema-schemata.html)

## 4. List tables and views in a schema

**Purpose:** Discover objects in the target schema. Replace the `'public'` filter string.

```sql
SELECT table_schema, table_name, table_type
FROM information_schema.tables
WHERE table_schema = 'public'
ORDER BY table_name;
```

**Result:** Schema, object name, and type within information_schema's visibility and object coverage. This does not promise every PostgreSQL-specific relation kind.

**Official documentation:** [information_schema.tables](https://www.postgresql.org/docs/17/infoschema-tables.html)

## 5. Inspect column definitions

**Purpose:** Inspect column order, types, nullability, defaults, and identity properties. Replace the schema and table filter strings.

```sql
SELECT ordinal_position, column_name, data_type, udt_name,
       is_nullable, column_default, is_identity, identity_generation
FROM information_schema.columns
WHERE table_schema = 'public' AND table_name = 'table_name'
ORDER BY ordinal_position;
```

**Result:** One row per column. `udt_name` helps identify arrays and concrete database types. This is column metadata, not a complete CREATE TABLE statement.

**Official documentation:** [information_schema.columns](https://www.postgresql.org/docs/17/infoschema-columns.html)

## 6. Inspect table constraints

**Purpose:** Read native definitions of primary keys, foreign keys, and other table constraints. Replace `'public.table_name'`; for quoted names use a string such as `'"Public"."Orders"'`.

```sql
SELECT conname AS constraint_name, contype AS constraint_type,
       pg_get_constraintdef(oid, true) AS definition
FROM pg_constraint
WHERE conrelid = 'public.table_name'::regclass
ORDER BY conname;
```

**Result:** One row per constraint. Common `contype` values are `p` (primary key), `f` (foreign key), `u` (unique), and `c` (check). The regclass cast fails if the target cannot be resolved.

**Official documentation:** [pg_constraint](https://www.postgresql.org/docs/17/catalog-pg-constraint.html) · [pg_get_constraintdef](https://www.postgresql.org/docs/17/functions-info.html)

## 7. Read index CREATE statements

**Purpose:** Obtain index names and CREATE INDEX definitions. Replace the schema and table filter strings.

```sql
SELECT indexname, indexdef
FROM pg_indexes
WHERE schemaname = 'public' AND tablename = 'table_name'
ORDER BY indexname;
```

**Result:** One row per index. When rebuilding, account for indexes owned by primary-key or unique constraints to avoid creating duplicates.

**Official documentation:** [pg_indexes](https://www.postgresql.org/docs/17/view-pg-indexes.html)

## 8. Read a view's query

**Purpose:** Obtain the SELECT query underlying a view. Replace `'public.view_name'`.

```sql
SELECT pg_get_viewdef('public.view_name'::regclass, true) AS view_query;
```

**Result:** The query body, without a complete CREATE VIEW wrapper, grants, or dependency objects.

**Official documentation:** [pg_get_viewdef](https://www.postgresql.org/docs/17/functions-info.html)

## 9. Read a specific function or procedure definition

**Purpose:** Obtain the CREATE OR REPLACE definition for one overload. Replace the qualified name and argument-type list, for example `'public.function_name(integer,text)'`.

```sql
SELECT pg_get_functiondef('public.function_name(integer)'::regprocedure) AS ddl;
```

**Result:** The selected function or procedure definition. Argument types disambiguate overloads. Aggregate and other object kinds are not interchangeable with ordinary functions here.

**Official documentation:** [pg_get_functiondef](https://www.postgresql.org/docs/17/functions-info.html) · [Object identifier types](https://www.postgresql.org/docs/17/datatype-oid.html)

## 10. Preview up to 100 rows

**Purpose:** Inspect a small sample. Replace the schema and table identifiers.

```sql
SELECT * FROM "public"."table_name" LIMIT 100;
```

**Result:** At most 100 rows. Add ORDER BY on a real primary key for repeatable ordering. For arrays or custom types returned as Base64 by the native worker, explicitly select `column_name::text` when a readable database representation is needed.

**Official documentation:** [SELECT and LIMIT](https://www.postgresql.org/docs/17/sql-select.html)

## Table DDL boundary

PostgreSQL has no general `SHOW CREATE TABLE`. The column, constraint, and index operations above explain those parts of a table; they are not complete table DDL. Full reconstruction can also require partitions, sequences, default-expression dependencies, ownership, and privileges. A dedicated schema export tool is appropriate for complete exports; this CLI's first version does not export SQL files.

Temporary tables and session settings only survive within a single CLI invocation. Separate calls are not one transaction.
