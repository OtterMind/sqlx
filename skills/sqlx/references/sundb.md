# SUNDB operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `table_name` and `column_name` with actual identifiers. SUNDB runs the Goldilocks engine, which folds unquoted identifiers to upper case, so a name created unquoted is stored in upper case and must be written that way; double quotes preserve exact case and let a keyword stand as a name. System catalog views sit under the `SYSTEM_` prefix.

`--type sundb` runs on the JDBC worker and builds `jdbc:goldilocks://host:port/database`, because the driver is the Goldilocks engine's and keeps its URL scheme. `--database` is the database name, the default database is `goldilocks`, the system user is `sys`, and the default system password is `gliese`; the default port is 22581. The driver is not bundled: run `sqlx driver add --type sundb --jar <path to goldilocks8.jar>` with the vendor's driver, which the SUNDB/Goldilocks image ships at `/goldilocks_home/lib/goldilocks8.jar`. The public vendor image's license expired in 2022, so testing against a real server needs a licensed installation. The vendor publishes manuals as PDFs rather than a browsable documentation site: the documentation root is https://www.sunjesoft.co.kr/en/, and the product site is https://www.sundb.com.cn/.

## 1. Confirm the connection and list the system accounts

**Purpose:** Check that the connection answers, then see which accounts exist. No placeholders need replacement.

```sql
SELECT 1 AS connected FROM DUAL;
SELECT * FROM SYSTEM_.USERS;
```

**Result:** One row for the first statement and one row per account for the second. `DUAL` is the one-row table a statement selects from when it needs a `FROM` clause but no data, and `SYSTEM_.USERS` is the account catalog of the Goldilocks engine.

**Official documentation:** [GOLDILOCKS 22c.1 User Manual](https://www.sunjesoft.co.kr/en/_files/ugd/216533_1b07b250f2c94a59a908abef0a0c5187.pdf)

## 2. List the tables the server exposes

**Purpose:** Discover the tables and views the catalog records. No placeholders need replacement.

```sql
SELECT * FROM SYSTEM_.TABLES;
```

**Result:** One row per table object, and the CLI prints the column list, which tells you which column carries the schema and which carries the table name. Read that column list before filtering, then restrict the next query by schema so the catalog is not read in full.

**Official documentation:** [GOLDILOCKS 22c.1 User Manual](https://www.sunjesoft.co.kr/en/_files/ugd/216533_1b07b250f2c94a59a908abef0a0c5187.pdf)

## 3. Inspect a table's columns

**Purpose:** Read the column catalog so the columns of one table can be seen. Replace `TABLE_NAME` once the catalog's table-name column is known, keeping its stored upper-case spelling.

```sql
SELECT * FROM SYSTEM_.COLUMNS;
```

**Result:** One row per column of every table. The CLI prints the column list, so add a predicate such as `WHERE TABLE_NAME = 'TABLE_NAME'` in the next run, using the name the output shows for the table column; upper-case it unless the table was created with a quoted name.

**Official documentation:** [GOLDILOCKS 3.2 User Manual](https://www.sunjesoft.co.kr/en/_files/ugd/216533_17279666fbdb415abc42c1087965007a.pdf)

## 4. Read a bounded page

**Purpose:** Read a page of rows without loading a whole table. Replace `table_name` and `column_name`.

```sql
SELECT column_name FROM table_name ORDER BY column_name LIMIT 100;
```

**Result:** Up to 100 rows. Goldilocks accepts `LIMIT`; a server that rejects it accepts the standard `FETCH FIRST 100 ROWS ONLY` instead. Always bound a read, because the same statement without a bound returns every row and the CLI then has to store that whole result.

**Official documentation:** [GOLDILOCKS 22c.1 User Manual](https://www.sunjesoft.co.kr/en/_files/ugd/216533_1b07b250f2c94a59a908abef0a0c5187.pdf)

## 5. Count rows

**Purpose:** Get a table's row count when a sample is not representative. Replace `table_name`.

```sql
SELECT COUNT(*) AS row_count FROM table_name;
```

**Result:** One row with the exact count, which reads the table. There is no separate statistics catalog to consult first, so decide from the table's size whether the count is worth running.

**Official documentation:** [GOLDILOCKS 3.2 User Manual](https://www.sunjesoft.co.kr/en/_files/ugd/216533_17279666fbdb415abc42c1087965007a.pdf)

## 6. Evaluate expressions with DUAL

**Purpose:** Run a statement that has no table, such as a connection check after a driver change. No placeholders need replacement.

```sql
SELECT CURRENT_TIMESTAMP AS now FROM DUAL;
```

**Result:** One row. `DUAL` works for a function probe as well as for `SELECT 1`, which is the cheapest way to confirm that a freshly registered `goldilocks8.jar` really connects before a longer query.

**Official documentation:** [GOLDILOCKS manuals](https://www.sunjesoft.co.kr/en/) · [SUNDB product site](https://www.sundb.com.cn/)
