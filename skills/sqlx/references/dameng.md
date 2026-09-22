# Dameng database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `SCHEMA_NAME`, `TABLE_NAME`, and `INDEX_NAME` with actual identifiers. Unquoted names are folded to upper case; double-quote an identifier when its stored case matters. Single-quoted SQL strings have separate escaping rules.

`--type dameng` (alias `dm`) connects through the vendor JDBC driver, `dm.jdbc.driver.DmDriver`, on port 5236. Dameng is Oracle-compatible: the account name doubles as the default schema, and a schema is usually an owner rather than a database. Official links target the Dameng product manual; match the connected server version when you look them up. The documentation root is https://eco.dameng.com/document/dm/en/.

## 1. Identify the current server and account

**Purpose:** Check the server version, the database name, and the account before operating on a target. No placeholders need replacement.

```sql
SELECT banner AS server_version FROM v$version;
SELECT name AS database_name FROM v$database;
SELECT USER AS current_user FROM dual;
```

**Result:** One row each: the server banner, for example `DM Database Server 64 V8`, the database name, and the upper-cased account name, which is also the default schema.

**Official documentation:** [Dameng product manual](https://eco.dameng.com/document/dm/en/)

## 2. List visible schemas

**Purpose:** Discover the owners whose objects the account can reach. No placeholders need replacement.

```sql
SELECT username FROM all_users ORDER BY username;
```

**Result:** One schema name per row. `all_users` lists every schema, while `user_users` lists the connected account only.

**Official documentation:** [Dameng product manual](https://eco.dameng.com/document/dm/en/)

## 3. List tables for an owner

**Purpose:** Discover queryable tables. Replace `SCHEMA_NAME` with the target owner.

```sql
SELECT owner, table_name
FROM all_tables
WHERE owner = 'SCHEMA_NAME'
ORDER BY table_name;
```

**Result:** One row per table with its owner. Use `user_tables` to list only the connected account's tables.

**Official documentation:** [Dameng product manual](https://eco.dameng.com/document/dm/en/)

## 4. Read a table's CREATE statement

**Purpose:** Obtain the DDL that recreates the table. Replace the identifiers.

```sql
SELECT DBMS_METADATA.GET_DDL('TABLE', 'TABLE_NAME', 'SCHEMA_NAME') AS ddl
FROM dual;
```

**Result:** One row with the CREATE TABLE statement, including storage and tablespace clauses.

**Official documentation:** [Dameng product manual](https://eco.dameng.com/document/dm/en/)

## 5. Inspect column definitions

**Purpose:** Inspect a table's columns, types, and defaults. Replace the identifiers.

```sql
SELECT column_id, column_name, data_type, data_length,
       data_precision, nullable, data_default
FROM all_tab_columns
WHERE owner = 'SCHEMA_NAME' AND table_name = 'TABLE_NAME'
ORDER BY column_id;
```

**Result:** One row per column in table order. Column names and types are reported in upper case.

**Official documentation:** [Dameng product manual](https://eco.dameng.com/document/dm/en/)

## 6. List a table's indexes

**Purpose:** List the indexes of a table before changing or dropping them. Replace the identifiers.

```sql
SELECT index_name, index_type, uniqueness
FROM all_indexes
WHERE table_owner = 'SCHEMA_NAME' AND table_name = 'TABLE_NAME'
ORDER BY index_name;
```

**Result:** One row per index with its type (`NORMAL`, `BITMAP`, and so on) and whether it is unique.

**Official documentation:** [Dameng product manual](https://eco.dameng.com/document/dm/en/)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the identifiers.

```sql
SELECT * FROM "SCHEMA_NAME"."TABLE_NAME" LIMIT 100;
```

**Result:** At most 100 rows. Without `ORDER BY` the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [Dameng product manual](https://eco.dameng.com/document/dm/en/)

Dameng keeps an Oracle-style catalog, so `all_tables`, `all_tab_columns` and `all_indexes` answer most metadata questions and `DBMS_METADATA` produces DDL. The connection has no separate database name in the URL, so the account decides the default schema. Each `--sql` argument is one statement, identifiers fold to upper case unless quoted, and an uncertain write must not be replayed automatically.
