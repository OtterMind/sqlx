# Trino database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Trino addresses objects with three-part names: `catalog.schema.table`. Replace `catalog`, `schema`, and `table_name` with actual identifiers. Quote identifiers with double quotes when they need case or special characters.

`--type trino` connects through the official JDBC driver. A username is required; a password is only sent when TLS is enabled. Set `--database <catalog>[.<schema>]` to choose the default catalog and schema for unqualified names. Official links target the current Trino documentation; check the connected server version first and select the matching documentation version when needed.

## 1. Identify the current connection

**Purpose:** Check the server version, current catalog, schema, and user before operating on a target. No placeholders need replacement.

```sql
SELECT version() AS server_version,
       current_catalog,
       current_schema,
       current_user;
```

**Result:** One context row. `current_schema` is NULL when no schema is selected.

**Official documentation:** [System functions](https://trino.io/docs/current/functions/system.html) · [Session functions](https://trino.io/docs/current/functions/session.html)

## 2. List available catalogs

**Purpose:** Discover the catalogs this cluster exposes. No placeholders need replacement.

```sql
SHOW CATALOGS;
```

**Result:** One catalog per row. Write support depends on the connector: `tpch`, `tpcds`, `jmx`, and `system` are read-only, while `memory` and database connectors accept writes when configured.

**Official documentation:** [SHOW CATALOGS](https://trino.io/docs/current/sql/show-catalogs.html)

## 3. List schemas in a catalog

**Purpose:** Discover schemas. Replace `catalog` with the target catalog.

```sql
SHOW SCHEMAS FROM catalog;
```

**Result:** One schema name per row.

**Official documentation:** [SHOW SCHEMAS](https://trino.io/docs/current/sql/show-schemas.html)

## 4. List tables in a schema

**Purpose:** Discover queryable tables and views. Replace `catalog` and `schema`.

```sql
SHOW TABLES FROM catalog.schema;
```

**Result:** One table name per row.

**Official documentation:** [SHOW TABLES](https://trino.io/docs/current/sql/show-tables.html)

## 5. Read a table's CREATE statement

**Purpose:** Obtain the connector-generated DDL for a table. Replace the catalog, schema, and table identifiers.

```sql
SHOW CREATE TABLE catalog.schema.table_name;
```

**Result:** The CREATE TABLE statement as the connector reports it. Connectors that cannot describe DDL return an error instead.

**Official documentation:** [SHOW CREATE TABLE](https://trino.io/docs/current/sql/show-create-table.html)

## 6. Inspect columns

**Purpose:** Inspect column names, types, and extra attributes. Replace the catalog, schema, and table identifiers.

```sql
DESCRIBE catalog.schema.table_name;
```

**Result:** One row per column with `Column`, `Type`, `Extra`, and `Comment`.

**Official documentation:** [DESCRIBE](https://trino.io/docs/current/sql/describe.html)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the catalog, schema, and table identifiers.

```sql
SELECT * FROM catalog.schema.table_name LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [SELECT](https://trino.io/docs/current/sql/select.html)

Trino runs one statement per request and has no transactions: a failed statement leaves earlier statements applied, and each catalog's connector decides which writes it accepts. `tpch` and `tpcds` exist for read-only testing, while `memory` accepts `CREATE TABLE`, `INSERT`, and `DELETE` for the lifetime of the coordinator.
