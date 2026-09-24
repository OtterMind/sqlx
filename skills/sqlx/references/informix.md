# IBM Informix operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `table_name`, `column_name`, and `owner_name` with actual identifiers. Unquoted identifiers are case-insensitive and are stored in lower case, so `Orders` reaches the table stored as `orders`; a double-quoted delimited identifier keeps its exact case and must then always be quoted.

`--type informix` runs on the JDBC worker and builds `jdbc:informix-sqli://host:port/database:INFORMIXSERVER=<server>;`, where `--database` is the Informix database name and the default port is 9088. The server instance name comes from `--service <server>`; without it the URL carries no `INFORMIXSERVER` and the driver refuses the connection. Informix is not on the list of engines whose driver SQLX may redistribute: run `sqlx driver add --type informix --jar <path to the vendor jdbc jar>` once, and `sqlx driver list` to see where each engine's driver comes from. A username and a password are required. The documentation root is https://www.ibm.com/docs/en/informix-servers.

## 1. Identify the connection

**Purpose:** Check the server version and the database the connection opened. No placeholders need replacement.

```sql
SELECT DBINFO('version', 'full') AS engine_version FROM systables WHERE tabid = 1;
SELECT DBINFO('dbname') AS database_name FROM systables WHERE tabid = 1;
```

**Result:** One row each. `systables` with `tabid = 1` is the single row every Informix expression-only select uses, because this engine has no `DUAL`; the row exists in every database.

**Official documentation:** [SYSTABLES](https://www.ibm.com/docs/en/informix-servers/14.10.0?topic=tables-systables)

## 2. List tables, views and sequences

**Purpose:** Discover the objects defined in the connected database. No placeholders need replacement.

```sql
SELECT tabname, owner, tabtype FROM systables WHERE tabtype IN ('T', 'V') ORDER BY tabname;
```

**Result:** One object per row, with `T` for a table, `V` for a view, `Q` for a sequence, and `E` for an external table. The catalog is per database, so this lists only the database named by `--database`.

**Official documentation:** [SYSTABLES](https://www.ibm.com/docs/en/informix-servers/14.10.0?topic=tables-systables)

## 3. Inspect a table's columns

**Purpose:** Read column names, type codes, and lengths for one table. Replace `table_name`.

```sql
SELECT c.colno, c.colname, c.coltype, c.collength
FROM syscolumns c, systables t
WHERE c.tabid = t.tabid AND t.tabname = 'table_name'
ORDER BY c.colno;
```

**Result:** One row per column. `coltype` is a numeric code, not a type name, and it is incremented by 256 when the column is `NOT NULL`, so a code such as 262 means a `SERIAL` column that cannot be null.

**Official documentation:** [SYSCOLUMNS](https://www.ibm.com/docs/en/informix-servers/14.10.0?topic=tables-syscolumns)

## 4. Read row counts and statistics

**Purpose:** Get an approximate row count without scanning the table. Replace `table_name`.

```sql
SELECT tabname, nrows, npused, ustlowts FROM systables WHERE tabname = 'table_name';
```

**Result:** One row. `nrows` and `npused` are estimates maintained by `UPDATE STATISTICS`, and `ustlowts` is when they were last recorded; without `UPDATE STATISTICS` the values are stale, so they must not be presented as an exact count.

**Official documentation:** [SYSTABLES](https://www.ibm.com/docs/en/informix-servers/14.10.0?topic=tables-systables)

## 5. Reconstruct a table's definition

**Purpose:** Describe a table when no `SHOW CREATE TABLE` exists here. Replace `table_name`.

```sql
SELECT c.colname, c.coltype, c.collength, t.ncols
FROM syscolumns c, systables t
WHERE c.tabid = t.tabid AND t.tabname = 'table_name'
ORDER BY c.colno;
```

**Result:** The catalog's view of the table, not DDL text. Informix generates DDL through the `dbschema` utility rather than through SQL, so a definition that includes constraints, indexes, and storage clauses has to come from that utility or from the other `sys*` catalogs.

**Official documentation:** [SYSCOLUMNS](https://www.ibm.com/docs/en/informix-servers/14.10.0?topic=tables-syscolumns)

## 6. Page large reads with SKIP and FIRST

**Purpose:** Read a page of rows without loading a whole table. Replace `table_name` and `column_name`.

```sql
SELECT SKIP 0 FIRST 100 column_name FROM table_name ORDER BY column_name;
```

**Result:** Up to 100 rows, and `SKIP` moves the window for the next page. Informix has no `LIMIT`; `FIRST` and `SKIP` are its row-limiting clauses, and both belong in the SELECT statement rather than appended to it.

**Official documentation:** [IBM Informix Servers documentation](https://www.ibm.com/docs/en/informix-servers)

## 7. Monitor the server through sysmaster

**Purpose:** Read server-wide state that the per-database catalog cannot answer. No placeholders need replacement.

```sql
SELECT * FROM sysmaster:syssqlstat;
SELECT * FROM sysmaster:syssessions;
```

**Result:** One row per statement or session on the whole server instance, not just this database. `sysmaster` is the shared monitoring database and is reached with the `sysmaster:` prefix; treat it as read-only and expect a large result, so bound it as in the previous section.

**Official documentation:** [IBM Informix Servers documentation](https://www.ibm.com/docs/en/informix-servers)
