# Oracle database operations

The datasource's `service` selects the Oracle service. A schema generally corresponds to an object owner. Changing the current schema does not change the service or login account.

Pass each SQL block as one `--command` argument. These standalone statements omit a trailing semicolon for JDBC. Semicolons required inside and at the end of a PL/SQL block remain part of that block. Do not submit SQL*Plus's `/` directive.

Replace `OWNER_NAME`, `TABLE_NAME`, and other placeholders. Names created without quotes normally appear in uppercase; preserve the exact stored case of quoted object names instead of uppercasing everything.

Official links target Oracle Database 19c. The documentation root is https://docs.oracle.com/en/database/oracle/oracle-database/19/index.html. Match the connected server version when you look them up.

## 1. Identify the database, service, schema, and session user

**Purpose:** Confirm the connection target. No placeholders need replacement.

```sql
SELECT SYS_CONTEXT('USERENV', 'DB_NAME') AS database_name,
       SYS_CONTEXT('USERENV', 'SERVICE_NAME') AS service_name,
       SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') AS schema_name,
       SYS_CONTEXT('USERENV', 'SESSION_USER') AS account_name
FROM dual
```

**Result:** One context row. Oracle services, PDBs, and schemas are different concepts; there is no interchangeable MySQL-style SHOW DATABASES operation.

**Official documentation:** [SYS_CONTEXT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/SYS_CONTEXT.html)

## 2. List visible users as candidate owners

**Purpose:** Discover potential object owners. No placeholders need replacement.

```sql
SELECT username
FROM all_users
ORDER BY username
```

**Result:** One visible user per row. A listed user need not own tables, and visibility does not grant access to all of that user's objects.

**Official documentation:** [ALL_USERS](https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_USERS.html)

## 3. List accessible tables for an owner

**Purpose:** Find the table to inspect or query. Replace `'OWNER_NAME'` with the owner's exact stored name.

```sql
SELECT owner, table_name
FROM all_tables
WHERE owner = 'OWNER_NAME'
ORDER BY table_name
```

**Result:** One accessible table per row. An empty result can reflect a case mismatch or insufficient privileges, not necessarily an owner with no tables.

**Official documentation:** [ALL_TABLES](https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_TABLES.html)

## 4. Read a table's CREATE statement

**Purpose:** Ask Oracle to generate a table's creation definition. Replace the second argument with the table name and the third with the owner. The first argument, `'TABLE'`, is an object type and must not be replaced with a table name.

```sql
SELECT DBMS_METADATA.GET_DDL('TABLE', 'TABLE_NAME', 'OWNER_NAME') AS ddl
FROM dual
```

**Result:** A CLOB containing DDL. Metadata access privileges are required, especially across owners. DBMS_METADATA transform settings affect the output. This is not a complete schema export including every related object and its data.

**Official documentation:** [DBMS_METADATA.GET_DDL](https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/DBMS_METADATA.html)

## 5. Inspect column definitions

**Purpose:** Inspect column order, types, length, numeric precision, nullability, and defaults. Replace owner and table names.

```sql
SELECT column_id, column_name, data_type, data_length,
       data_precision, data_scale, nullable, data_default
FROM all_tab_columns
WHERE owner = 'OWNER_NAME' AND table_name = 'TABLE_NAME'
ORDER BY column_id
```

**Result:** One row per column. `data_length` is measured in bytes and must not automatically be treated as a character count. `data_precision` and `data_scale` describe numeric precision and fractional digits.

**Official documentation:** [ALL_TAB_COLUMNS](https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_TAB_COLUMNS.html)

## 6. List a table's indexes

**Purpose:** Identify index names, their owners, and uniqueness. Replace the table owner and table name.

```sql
SELECT owner AS index_owner, index_name, uniqueness
FROM all_indexes
WHERE table_owner = 'OWNER_NAME' AND table_name = 'TABLE_NAME'
ORDER BY index_name
```

**Result:** One row per index. `UNIQUE` indicates a unique index. This operation does not list index columns; use the next operation for those.

**Official documentation:** [ALL_INDEXES](https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_INDEXES.html)

## 7. Inspect index columns and their order

**Purpose:** Understand composite-index column order and sort direction. Replace the table owner and table name.

```sql
SELECT index_owner, index_name, column_position, column_name, descend
FROM all_ind_columns
WHERE table_owner = 'OWNER_NAME' AND table_name = 'TABLE_NAME'
ORDER BY index_owner, index_name, column_position
```

**Result:** One row per index column. Function-based indexes can reference generated column names; obtain expressions from `all_ind_expressions` or the index DDL when necessary.

**Official documentation:** [ALL_IND_COLUMNS](https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_IND_COLUMNS.html) · [ALL_IND_EXPRESSIONS](https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/ALL_IND_EXPRESSIONS.html)

## 8. Preview up to 100 rows

**Purpose:** Inspect a small sample on Oracle 12c or later. Replace the quoted owner and table identifiers.

```sql
SELECT * FROM "OWNER_NAME"."TABLE_NAME" FETCH FIRST 100 ROWS ONLY
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real primary key for a repeatable sample.

**Official documentation:** [SELECT and row limiting](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/SELECT.html)

Oracle DDL has implicit-commit behavior. A later statement failure does not prove earlier changes rolled back.
