# XuguDB operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `OWNER_NAME`, `TABLE_NAME`, and `COLUMN_NAME` with actual identifiers. XuguDB folds unquoted identifiers to upper case, so a name created unquoted is stored in upper case and is only reachable in that spelling; keep the stored case when you write the query.

`--type xugu` runs on the JDBC worker with the driver `com.xugu.cloudjdbc.Driver`, which builds `jdbc:xugu://host:port/database`. `--database` is the database name, and `SYSTEM` is the system database; the default port is 5138. A username and a password are required, and the account must exist in the database being opened. XuguDB presents Oracle-style catalogs here: `ALL_TABLES`, `ALL_TAB_COLUMNS`, `USER_TABLES`, and `DUAL`. The vendor documentation root is https://www.xugudb.com/, and the documentation site is https://help.xugudb.com/.

## 1. List the schemas the account can see

**Purpose:** Find the owners whose tables are visible, before naming one. No placeholders need replacement.

```sql
SELECT DISTINCT OWNER AS schema_name FROM ALL_TABLES ORDER BY OWNER;
```

**Result:** One owner per row. This is the account's visible set, not every schema in the database: a schema whose tables are not granted to the account does not appear.

**Official documentation:** [ALL_TABLES](https://help.xugudb.com/content/reference/system-view/all/all_tables) · [Schemas](https://help.xugudb.com/content/reference/object/schema)

## 2. List tables in a schema

**Purpose:** Find the table to inspect or query. Replace `OWNER_NAME`, or drop the filter to use `USER_TABLES` for the account's own tables.

```sql
SELECT OWNER, TABLE_NAME FROM ALL_TABLES WHERE OWNER = 'OWNER_NAME' ORDER BY TABLE_NAME;
SELECT TABLE_NAME FROM USER_TABLES ORDER BY TABLE_NAME;
```

**Result:** One table per row. `ALL_TABLES` covers the tables the account may read, `USER_TABLES` only those it owns. An empty `ALL_TABLES` result can mean a case mismatch in `OWNER_NAME` or missing privileges.

**Official documentation:** [ALL_TABLES](https://help.xugudb.com/content/reference/system-view/all/all_tables) · [Tables](https://help.xugudb.com/content/reference/object/table/introduction)

## 3. Inspect a table's columns

**Purpose:** Read column names, types, lengths, and nullability of one table. Replace `OWNER_NAME` and `TABLE_NAME`.

```sql
SELECT COLUMN_ID, COLUMN_NAME, DATA_TYPE, DATA_LENGTH, DATA_PRECISION, DATA_SCALE, NULLABLE
FROM ALL_TAB_COLUMNS
WHERE OWNER = 'OWNER_NAME' AND TABLE_NAME = 'TABLE_NAME'
ORDER BY COLUMN_ID;
```

**Result:** One row per column in declaration order. The catalog views keep Oracle's column names, and `DATA_LENGTH` is a byte length, so it must not be read as a character count for a character column.

**Official documentation:** [System views](https://help.xugudb.com/content/reference/system-view/all/all_tables) · [Tables](https://help.xugudb.com/content/reference/object/table/introduction)

## 4. Read a bounded page

**Purpose:** Read rows without loading a whole table, with a repeatable order. Replace `OWNER_NAME`, `TABLE_NAME`, and `COLUMN_NAME`.

```sql
SELECT COLUMN_NAME FROM OWNER_NAME.TABLE_NAME ORDER BY COLUMN_NAME LIMIT 100 OFFSET 0;
```

**Result:** Up to 100 rows. The result-set clause also accepts `FETCH FIRST n ROWS ONLY`, so a query copied from Oracle usually runs. Names are resolved case-sensitively against the stored upper-case spelling unless the identifier is quoted.

**Official documentation:** [Restricting a result set](https://help.xugudb.com/content/reference/sql/select/resultset-restricted) · [SELECT](https://help.xugudb.com/content/reference/sql/select/select)

## 5. Count rows

**Purpose:** Get a table's row count when the sample is not representative. Replace `OWNER_NAME` and `TABLE_NAME`.

```sql
SELECT COUNT(*) AS row_count FROM OWNER_NAME.TABLE_NAME;
```

**Result:** One row with the exact count, which reads the table. Use the catalog instead when an estimate is enough, or the table is large.

**Official documentation:** [SELECT](https://help.xugudb.com/content/reference/sql/select/select)

## 6. Evaluate an expression with DUAL

**Purpose:** Run a statement that has no table, such as a connection check or a function probe. No placeholders need replacement.

```sql
SELECT 1 AS connected FROM DUAL;
```

**Result:** One row. `DUAL` is the single-row table a query uses when it needs a `FROM` clause but no data, which is the Oracle spelling an agent should expect here.

**Official documentation:** [FROM clause](https://help.xugudb.com/content/reference/sql/select/from) · [Databases](https://help.xugudb.com/content/reference/object/database)
