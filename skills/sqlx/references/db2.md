# IBM Db2 operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `SCHEMA_NAME`, `TABLE_NAME`, and `COLUMN_NAME` with actual identifiers. Db2 folds unquoted identifiers to upper case, so an unquoted name is stored in upper case and is only reachable in that spelling; double quotes preserve the exact case and let a keyword stand as a name.

`--type db2` runs on the JDBC worker and builds `jdbc:db2://host:port/database`, where `--database` is the database name and the default port is 50000. A username and a password are required. IBM's driver is not bundled: run `sqlx driver add --type db2 --jar <path to the jcc jar>` once with the driver from IBM, and `sqlx driver list` to see where each engine's driver comes from. Schemas are objects inside the connected database, not separate databases. The documentation root is https://www.ibm.com/docs/en/db2.

## 1. Identify the current connection

**Purpose:** Check the database, the authorization ID, and the current schema before operating on a target. No placeholders need replacement.

```sql
SELECT CURRENT SERVER AS database_name,
       CURRENT USER AS authorization_id,
       CURRENT SCHEMA AS schema_name
FROM SYSIBM.SYSDUMMY1;
```

**Result:** One context row. `SYSIBM.SYSDUMMY1` is the one-row dummy table a statement selects from when it has no real table. `CURRENT SCHEMA` is what unqualified names resolve against, and it is not the same as the authorization ID.

**Official documentation:** [SELECT statement](https://www.ibm.com/docs/en/db2/11.5?topic=statements-select)

## 2. List schemas

**Purpose:** Discover the schemas that hold tables the account can see. No placeholders need replacement.

```sql
SELECT DISTINCT TABSCHEMA AS schema_name FROM SYSCAT.TABLES ORDER BY TABSCHEMA;
```

**Result:** One schema per row. `SYSCAT` holds the catalog views and is readable by any account with access to the database; the system schemas such as `SYSIBM` and `SYSCAT` appear alongside the user schemas.

**Official documentation:** [SYSCAT.TABLES](https://www.ibm.com/docs/en/db2/11.5?topic=views-syscattables)

## 3. List tables and views

**Purpose:** Find the table to inspect or query. Replace `SCHEMA_NAME`.

```sql
SELECT TABSCHEMA, TABNAME, TYPE FROM SYSCAT.TABLES
WHERE TABSCHEMA = 'SCHEMA_NAME' AND TYPE IN ('T', 'V')
ORDER BY TABNAME;
```

**Result:** One table or view per row, with `TYPE` `T` for a table and `V` for a view. Restrict `TABSCHEMA` rather than scanning the whole catalog, which on a large database is slow.

**Official documentation:** [SYSCAT.TABLES](https://www.ibm.com/docs/en/db2/11.5?topic=views-syscattables)

## 4. Inspect a table's columns

**Purpose:** Read column names, types, lengths, nullability, and defaults. Replace `SCHEMA_NAME` and `TABLE_NAME`.

```sql
SELECT COLNO, COLNAME, TYPENAME, LENGTH, SCALE, NULLS, KEYSEQ, DEFAULT
FROM SYSCAT.COLUMNS
WHERE TABSCHEMA = 'SCHEMA_NAME' AND TABNAME = 'TABLE_NAME'
ORDER BY COLNO;
```

**Result:** One row per column. `COLNO` starts at 0, `NULLS` is `Y` or `N`, `KEYSEQ` is the position inside the primary key, and `DEFAULT` is a CLOB, so a long default may need a cast to read comfortably.

**Official documentation:** [SYSCAT.COLUMNS](https://www.ibm.com/docs/en/db2/11.5?topic=views-syscatcolumns)

## 5. Read row counts and statistics

**Purpose:** Get a table's row count and when its statistics were collected, without scanning it. Replace `SCHEMA_NAME` and `TABLE_NAME`.

```sql
SELECT TABSCHEMA, TABNAME, CARD, STATS_TIME FROM SYSCAT.TABLES
WHERE TABSCHEMA = 'SCHEMA_NAME' AND TABNAME = 'TABLE_NAME';
```

**Result:** One row. `CARD` is the row count recorded by `RUNSTATS` and is `-1` when statistics were never collected, so it must not be reported as the real row count without checking it. `STATS_TIME` is when they were last collected.

**Official documentation:** [SYSCAT.TABLES](https://www.ibm.com/docs/en/db2/11.5?topic=views-syscattables)

## 6. Reconstruct a table's definition

**Purpose:** Describe a table when a `SHOW CREATE TABLE`-style statement does not exist here. Replace `SCHEMA_NAME` and `TABLE_NAME`.

```sql
SELECT COLNAME, TYPENAME, LENGTH, SCALE, NULLS, DEFAULT FROM SYSCAT.COLUMNS
WHERE TABSCHEMA = 'SCHEMA_NAME' AND TABNAME = 'TABLE_NAME' ORDER BY COLNO;
```

**Result:** The column list a CREATE TABLE would repeat, but not the DDL text: Db2 has no in-SQL statement that returns the table's CREATE. The `db2look` tool shipped with the server generates the DDL, and constraint, index, and partition details come from other catalog views such as `SYSCAT.KEYCOLUSE`, `SYSCAT.INDEXES`, and `SYSCAT.DATAPARTITIONS`.

**Official documentation:** [SYSCAT.COLUMNS](https://www.ibm.com/docs/en/db2/11.5?topic=views-syscatcolumns) · [SYSCAT.TABLES](https://www.ibm.com/docs/en/db2/11.5?topic=views-syscattables)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace `SCHEMA_NAME` and `TABLE_NAME`.

```sql
SELECT * FROM SCHEMA_NAME.TABLE_NAME FETCH FIRST 100 ROWS ONLY;
```

**Result:** At most 100 rows. Db2 has no `LIMIT`; `FETCH FIRST n ROWS ONLY` is the spelling it accepts, and `ORDER BY` before it is what makes the sample repeatable.

**Official documentation:** [SELECT statement](https://www.ibm.com/docs/en/db2/11.5?topic=statements-select)
