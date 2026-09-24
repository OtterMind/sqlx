# Apache Kylin operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `project`, `table_name`, and `column_name` with actual identifiers. Kylin converts unquoted identifiers to upper case before matching, and double quotes preserve the exact case as well as escape a keyword used as a name. String literals use single quotes.

`--type kylin` runs on the JDBC worker with the driver `org.apache.kylin.jdbc.Driver`, which builds `jdbc:kylin://host:port/project`. `--database` carries the Kylin project name, for example `learn_kylin`, not a database in the relational sense; the default port is 7070. The default credentials are `ADMIN`/`KYLIN`, and they should be changed on a real deployment. Kylin is an OLAP engine over pre-built cubes: it answers `SELECT` with the ANSI dialect, exposes tables with `SHOW TABLES`, and accepts neither writes nor DDL. Official links target the current Kylin documentation, whose root is https://kylin.apache.org/.

## 1. Confirm the project and the query engine

**Purpose:** Check that the project named by `--database` accepts queries before reading a cube. No placeholders need replacement.

```sql
SELECT 1 AS connected;
```

**Result:** One row. A failure here means the project name, the credentials, or the JDBC port is wrong; the statement itself needs no table and no cube.

**Official documentation:** [SQL specification](https://kylin.apache.org/docs/query/specification/sql_spec/)

## 2. List the tables the project exposes

**Purpose:** Discover the tables the project and its models make queryable. No placeholders need replacement.

```sql
SHOW TABLES;
```

**Result:** One table name per row from the project's data source and internal tables. A table that appears here is not guaranteed to be answerable from a cube; a query that matches no cube falls back to the data source or is refused. Not every Kylin build answers this: a 5.0.2 image rejects it with "This SQL is not supported at the moment", and the tables to query then come from the cube's model in the Kylin interface.

**Official documentation:** [Internal table](https://kylin.apache.org/docs/internaltable/intro) · [Datasource](https://kylin.apache.org/docs/datasource/intro)

## 3. Read a bounded sample to learn the columns

**Purpose:** See the column names and the shape of one table without pulling a whole cube. Replace `table_name`.

```sql
SELECT * FROM table_name LIMIT 5;
```

**Result:** At most five rows, and the CLI prints the column list of the result, which is how an agent learns a table's columns here. Do not assume a `DESCRIBE` statement: read the column names from a sample, or from the model that defines the cube in the Kylin interface.

**Official documentation:** [Query](https://kylin.apache.org/docs/query/insight) · [SELECT syntax](https://kylin.apache.org/docs/query/specification/sql_spec/)

## 4. Count and aggregate rows

**Purpose:** Read a single number that Kylin can answer from a cube. Replace `table_name` and `column_name`.

```sql
SELECT COUNT(*) AS row_count FROM table_name;
SELECT column_name, COUNT(*) AS row_count FROM table_name GROUP BY column_name;
```

**Result:** One row per group. Aggregations over cube dimensions and measures are what Kylin is built for; a query whose grouping does not match a cube is answered by pushing the work down to the data source or rejected, so it can be far slower than the same query on a matching cube.

**Official documentation:** [Basic functions](https://kylin.apache.org/docs/query/specification/functions) · [Query pushdown](https://kylin.apache.org/docs/query/push_down)

## 5. Escape keywords and preserve exact case

**Purpose:** Query a column whose name collides with a Kylin keyword. Replace `YEAR` and `DATES` with the real names.

```sql
SELECT "YEAR" FROM DATES LIMIT 5;
```

**Result:** One row per matching record. Without the double quotes the query fails with a parse error, because the engine cannot tell the column from the keyword. The same quoting rule applies when a name was created with mixed case.

**Official documentation:** [Identifiers and escaping](https://kylin.apache.org/docs/query/specification/sql_spec/)

## 6. Recognise that writes and DDL are rejected

**Purpose:** Understand what happens before an agent tries to change data. No placeholders need replacement.

```sql
CREATE TABLE sqlx_probe (id INTEGER);
INSERT INTO table_name (column_name) VALUES ('value');
DELETE FROM table_name WHERE id = 1;
```

**Result:** All three statements fail: Kylin is a query engine, and the JDBC endpoint accepts only the `SELECT` grammar. Nothing is created, written, or deleted. Build or refresh the cube in the Kylin web interface or its REST API instead, then query it again.

**Official documentation:** [SQL specification](https://kylin.apache.org/docs/query/specification/sql_spec/) · [REST API](https://kylin.apache.org/docs/restapi/intro)
