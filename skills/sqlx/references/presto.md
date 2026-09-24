# Presto operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Presto addresses objects with three-part names: `catalog.schema.table`. Replace `catalog`, `schema`, and `table_name` with actual identifiers. Quote identifiers with double quotes when they need case or special characters, and keep string literals in single quotes.

`--type presto` runs on the JDBC worker with the PrestoDB driver `com.facebook.presto.jdbc.PrestoDriver`, which builds `jdbc:presto://host:port/catalog/schema`. `--database` carries `catalog.schema`, and each dot becomes a slash in that URL; the default port is 8080. A username is required; a password is only sent when TLS is enabled. This type is PrestoDB, not Trino: the two are separate products with separate drivers, the drivers are not interchangeable, and a Trino server is not reachable through `--type presto` (use `references/trino.md` for Trino). Official links target the current PrestoDB documentation, whose root is https://prestodb.io/docs/current/.

## 1. Identify the current connection

**Purpose:** Check the server version, the current catalog, schema, and user before operating on a target. No placeholders need replacement.

```sql
SELECT version() AS server_version,
       current_catalog,
       current_schema,
       current_user;
```

**Result:** One context row. `current_schema` is NULL when the connection selected no schema.

**Official documentation:** [System functions](https://prestodb.io/docs/current/functions/system.html) · [Session information functions](https://prestodb.io/docs/current/functions/session.html)

## 2. List available catalogs

**Purpose:** Discover the catalogs this cluster exposes. No placeholders need replacement.

```sql
SHOW CATALOGS;
```

**Result:** One catalog per row. Write support depends on the connector: `tpch`, `tpcds`, `jmx`, and `system` are read-only, while a database connector accepts writes only when the cluster configured it.

**Official documentation:** [SHOW CATALOGS](https://prestodb.io/docs/current/sql/show-catalogs.html)

## 3. List schemas and tables

**Purpose:** Discover schemas, then the queryable tables and views inside one of them. Replace `catalog` and `schema`.

```sql
SHOW SCHEMAS FROM catalog;
SHOW TABLES FROM catalog.schema;
```

**Result:** One schema name per row for the first statement, one table name per row for the second. Both report one catalog only; neither spans catalogs. `information_schema` and the connector's own schemas appear alongside the data schemas.

**Official documentation:** [SHOW SCHEMAS](https://prestodb.io/docs/current/sql/show-schemas.html) · [SHOW TABLES](https://prestodb.io/docs/current/sql/show-tables.html)

## 4. Inspect columns

**Purpose:** Read column names, types, and extra attributes for one table. Replace the catalog, schema, and table identifiers.

```sql
DESCRIBE catalog.schema.table_name;
```

**Result:** One row per column with `Column`, `Type`, `Extra`, and `Comment`. Partition columns are listed like ordinary columns.

**Official documentation:** [DESCRIBE](https://prestodb.io/docs/current/sql/describe.html)

## 5. Read a table's CREATE statement

**Purpose:** Obtain the connector-generated DDL for a table. Replace the catalog, schema, and table identifiers.

```sql
SHOW CREATE TABLE catalog.schema.table_name;
```

**Result:** The CREATE TABLE statement as the connector reports it. Connectors that cannot describe DDL, such as `tpch`, return an error instead.

**Official documentation:** [SHOW CREATE TABLE](https://prestodb.io/docs/current/sql/show-create-table.html)

## 6. Count rows without scanning the table

**Purpose:** Get a row count and column statistics cheaply. Replace the catalog, schema, and table identifiers.

```sql
SHOW STATS FOR catalog.schema.table_name;
```

**Result:** One row per column plus a summary row whose `row_count` is the table's row count. Only connectors that maintain table statistics fill it in; a NULL `row_count` means the connector has no statistics, so fall back to `SELECT count(*)`. This is not the same statement as Trino's.

**Official documentation:** [SHOW STATS](https://prestodb.io/docs/current/sql/show-stats.html) · [SELECT](https://prestodb.io/docs/current/sql/select.html)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the catalog, schema, and table identifiers.

```sql
SELECT * FROM catalog.schema.table_name LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real key column when a repeatable sample matters. Always bound a preview, because a Presto query reads from remote connectors.

**Official documentation:** [SELECT](https://prestodb.io/docs/current/sql/select.html)
