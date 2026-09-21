# SQL Server database operations

These operations run in the datasource's selected database. Submit each block as one `--sql` argument. `GO` is a client batch separator and must not be sent to JDBC.

`dbo` is an example schema. Replace it and placeholders such as `table_name` and `view_or_procedure`. Brackets delimit identifiers, with embedded `]` doubled. `N'...'` denotes a Unicode SQL string.

Links point to official Microsoft Learn pages. Match the connected server version when you look them up.

## 1. Identify the current connection

**Purpose:** Confirm the server version, selected database, default schema, and original login. No placeholders need replacement.

```sql
SELECT @@VERSION AS server_version,
       DB_NAME() AS database_name,
       SCHEMA_NAME() AS default_schema,
       ORIGINAL_LOGIN() AS login_name;
```

**Result:** One context row. The default schema is not the only schema the account can access.

**Official documentation:** [DB_NAME](https://learn.microsoft.com/en-us/sql/t-sql/functions/db-name-transact-sql) · [SCHEMA_NAME](https://learn.microsoft.com/en-us/sql/t-sql/functions/schema-name-transact-sql) · [ORIGINAL_LOGIN](https://learn.microsoft.com/en-us/sql/t-sql/functions/original-login-transact-sql)

## 2. List visible databases

**Purpose:** Find the intended database. No placeholders need replacement.

```sql
SELECT name AS database_name, state_desc
FROM sys.databases
ORDER BY name;
```

**Result:** Database names and states. Permissions affect visibility. A listed database may still be inaccessible, and a non-ONLINE state requires investigation.

**Official documentation:** [sys.databases](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-databases-transact-sql)

## 3. List schemas in the current database

**Purpose:** Discover namespaces for tables and other objects. No placeholders need replacement.

```sql
SELECT name AS schema_name
FROM sys.schemas
ORDER BY name;
```

**Result:** Schema names, including system schemas.

**Official documentation:** [sys.schemas](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/schemas-catalog-views-sys-schemas)

## 4. List user tables in a schema

**Purpose:** Find a target table. Replace the `N'dbo'` filter string with the actual schema.

```sql
SELECT s.name AS schema_name, t.name AS table_name
FROM sys.tables AS t
JOIN sys.schemas AS s ON s.schema_id = t.schema_id
WHERE s.name = N'dbo'
ORDER BY t.name;
```

**Result:** One visible user table per row. This does not include views; query `sys.views` for view discovery.

**Official documentation:** [sys.tables](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-tables-transact-sql)

## 5. Inspect columns, defaults, and identity properties

**Purpose:** Inspect a table's field definitions. Replace `N'dbo.table_name'` with the actual qualified table name.

```sql
SELECT c.column_id, c.name AS column_name,
       TYPE_NAME(c.user_type_id) AS data_type,
       c.max_length, c.precision, c.scale,
       c.is_nullable, c.is_identity, c.is_computed,
       dc.definition AS default_definition
FROM sys.columns AS c
LEFT JOIN sys.default_constraints AS dc ON dc.object_id = c.default_object_id
WHERE c.object_id = OBJECT_ID(N'dbo.table_name')
ORDER BY c.column_id;
```

**Result:** One row per column. `max_length` is generally in bytes, and `-1` denotes a MAX type; an `nvarchar` byte length is not its character count. Empty results can reflect a wrong name or missing metadata permissions.

**Official documentation:** [sys.columns](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-columns-transact-sql) · [sys.default_constraints](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-default-constraints-transact-sql)

## 6. Inspect index properties

**Purpose:** Read index type, uniqueness, primary-key ownership, unique-constraint ownership, and filter definitions. Replace the qualified table name.

```sql
SELECT name AS index_name, type_desc, is_unique,
       is_primary_key, is_unique_constraint, filter_definition
FROM sys.indexes
WHERE object_id = OBJECT_ID(N'dbo.table_name') AND index_id > 0
ORDER BY index_id;
```

**Result:** One row per index, excluding the heap entry. Index columns are returned by the next operation.

**Official documentation:** [sys.indexes](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-indexes-transact-sql)

## 7. Inspect index key and included columns

**Purpose:** Understand composite-key order and distinguish INCLUDE columns. Replace the qualified table name.

```sql
SELECT i.name AS index_name, c.name AS column_name,
       ic.key_ordinal, ic.index_column_id,
       ic.is_descending_key, ic.is_included_column
FROM sys.indexes AS i
JOIN sys.index_columns AS ic
  ON ic.object_id = i.object_id AND ic.index_id = i.index_id
JOIN sys.columns AS c
  ON c.object_id = ic.object_id AND c.column_id = ic.column_id
WHERE i.object_id = OBJECT_ID(N'dbo.table_name') AND i.index_id > 0
ORDER BY i.index_id, ic.index_column_id;
```

**Result:** One row per associated index column. For ordinary rowstore indexes, `key_ordinal` gives key position and `is_included_column` identifies INCLUDE columns. Interpret special index kinds together with their index type.

**Official documentation:** [sys.index_columns](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-index-columns-transact-sql)

## 8. Read a view, procedure, or function definition

**Purpose:** Obtain the definition of a supported SQL module. Replace `N'dbo.view_or_procedure'` with its qualified name.

```sql
SELECT OBJECT_DEFINITION(OBJECT_ID(N'dbo.view_or_procedure')) AS definition;
```

**Result:** Module definition text. A missing object, insufficient permissions, or an encrypted module can yield NULL. **This operation does not return CREATE TABLE for an ordinary table.**

**Official documentation:** [OBJECT_DEFINITION](https://learn.microsoft.com/en-us/sql/t-sql/functions/object-definition-transact-sql)

## 9. Preview up to 100 rows

**Purpose:** Inspect a small data sample. Replace the schema and table identifiers.

```sql
SELECT TOP (100) * FROM [dbo].[table_name];
```

**Result:** At most 100 rows. Add ORDER BY on a real primary key when stable ordering is required.

**Official documentation:** [TOP](https://learn.microsoft.com/en-us/sql/t-sql/queries/top-transact-sql)

## Table DDL boundary

Ordinary tables do not have an OBJECT_DEFINITION result containing CREATE TABLE. Complete reconstruction requires column types, identity/computed/default definitions, constraints, indexes, and other attributes. The catalog operations above describe their respective parts and are not a complete export.

The JDBC worker's initial authentication profile uses a database username/password and verifies TLS certificates by default. `USE [database]` only affects statements in the same CLI invocation; a datasource pointing directly at the intended database is usually preferable. Integrated Windows authentication is not part of the first verified authentication profile.
