# GBase 8s operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `table_name`, `column_name`, and `owner_name` with actual identifiers. Unquoted identifiers are case-insensitive and are stored in lower case, so `Orders` reaches the table stored as `orders`; a double-quoted delimited identifier keeps its exact case and must then always be quoted.

`--type gbase8s` runs on the JDBC worker and builds `jdbc:gbasedbt-sqli://host:port/database:GBASEDBTSERVER=<server>;`, where `--database` is the database name and the default port is 9088. The server instance name comes from `--service <server>`; without it the driver answers `GBASEDBTSERVER has to be specified` and the connection never opens. The driver is not bundled: run `sqlx driver add --type gbase8s --jar <path to the vendor driver jar>`, and `sqlx driver list` to see where each engine's driver comes from. The vendor ships a wrapper jar that contains the real `ifxjdbc.jar`, and the file to provide is that inner jar, whose driver class is `com.gbasedbt.jdbc.IfxDriver`. A username and a password are required. GBase 8s is Informix-compatible, so it uses the same `systables` and `syscolumns` catalogs and the same `SELECT ... FROM systables WHERE tabid = 1` idiom. Two limits come from the driver rather than the server: it reads a `VARCHAR` column back empty, so store varying text in `LVARCHAR`, and it does not implement the JDBC connection and multi-result calls SQLX probes with, which the worker tolerates. The documentation root is https://www.gbase.cn/, and the product documentation set is at https://docs.gbasedbt.com/gbase8s/.

## 1. Identify the connection

**Purpose:** Check the server version and the database the connection opened. No placeholders need replacement.

```sql
SELECT DBINFO('version', 'full') AS engine_version FROM systables WHERE tabid = 1;
SELECT DBINFO('dbname') AS database_name FROM systables WHERE tabid = 1;
```

**Result:** One row each. `systables` with `tabid = 1` is the single row an expression-only select uses, because this engine has no `DUAL`; the row exists in every database, so the statement also proves the database was reached.

**Official documentation:** [SYSTABLES](https://docs.gbasedbt.com/gbase8s/sqr/ids_sqr_072.html)

## 2. List tables, views and sequences

**Purpose:** Discover the objects defined in the connected database. No placeholders need replacement.

```sql
SELECT tabname, owner, tabtype FROM systables WHERE tabtype IN ('T', 'V') ORDER BY tabname;
```

**Result:** One object per row, with `T` for a table, `V` for a view, `Q` for a sequence, and `E` for an external table. The system catalog tables appear with the user tables; their `tabid` is below 100, while a user object's `tabid` starts at 100.

**Official documentation:** [SYSTABLES](https://docs.gbasedbt.com/gbase8s/sqr/ids_sqr_072.html)

## 3. Inspect a table's columns

**Purpose:** Read column names, type codes, and lengths for one table. Replace `table_name`.

```sql
SELECT c.colno, c.colname, c.coltype, c.collength
FROM syscolumns c, systables t
WHERE c.tabid = t.tabid AND t.tabname = 'table_name'
ORDER BY c.colno;
```

**Result:** One row per column, in the order the columns were defined. `coltype` is a numeric code, and it is incremented by 256 when the column is `NOT NULL`, so a code such as 262 means a `SERIAL` column that cannot be null.

**Official documentation:** [SYSCOLUMNS](https://docs.gbasedbt.com/gbase8s/sqr/ids_sqr_025.html)

## 4. Read row counts and statistics

**Purpose:** Get an approximate row count without scanning the table. Replace `table_name`.

```sql
SELECT tabname, nrows, npused, ustlowts FROM systables WHERE tabname = 'table_name';
```

**Result:** One row. `nrows` and `npused` are estimates that `UPDATE STATISTICS` maintains, and `ustlowts` is when they were last recorded; a table whose statistics were never collected reports stale values that must not be presented as an exact count.

**Official documentation:** [SYSTABLES](https://docs.gbasedbt.com/gbase8s/sqr/ids_sqr_072.html) · [Updating catalog statistics](https://docs.gbasedbt.com/gbase8s/sqr/ids_sqr_013.html)

## 5. Reconstruct a table's definition

**Purpose:** Describe a table when no `SHOW CREATE TABLE` exists here. Replace `table_name`.

```sql
SELECT c.colname, c.coltype, c.collength, t.ncols
FROM syscolumns c, systables t
WHERE c.tabid = t.tabid AND t.tabname = 'table_name'
ORDER BY c.colno;
```

**Result:** The catalog's view of the table, not DDL text. GBase 8s generates DDL through the `dbschema` utility rather than through SQL, so a definition that includes constraints, indexes, and storage clauses comes from that utility or from the other `sys*` catalogs.

**Official documentation:** [Using the system catalog](https://docs.gbasedbt.com/gbase8s/sqr/ids_sqr_011.html)

## 6. Read a bounded page with FIRST

**Purpose:** Read a page of rows without loading a whole table. Replace `table_name` and `column_name`.

```sql
SELECT FIRST 100 column_name FROM table_name ORDER BY column_name;
```

**Result:** Up to 100 rows. `FIRST` and `SKIP` are this engine's row-limiting options and belong inside the SELECT statement; `SKIP 100 FIRST 100` reads the second page. There is no `LIMIT` here, so a query copied from another engine usually has to be rewritten.

**Official documentation:** [FIRST option](https://docs.gbasedbt.com/gbase8s/sqs/ids_sqs_0984.html) · [FIRST and SKIP as column names](https://docs.gbasedbt.com/gbase8s/sqs/ids_sqs_0986.html)
