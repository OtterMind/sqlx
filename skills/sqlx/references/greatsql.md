# GreatSQL database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when operations need to share connection state.

Replace `database_name`, `table_name`, `view_name`, and `procedure_name` with actual identifiers. Quote identifiers with backticks and double embedded backticks. Single-quoted SQL strings have separate escaping rules.

GreatSQL is a MySQL-compatible fork, so `--type greatsql` uses the MySQL worker and a `mysql` datasource reaches the same server. It serves the MySQL protocol on port 3306 by default. Official links target the current GreatSQL documentation; match the connected server version when you look them up. The documentation root is https://greatsql.cn/docs/.

## 1. Identify the current connection

**Purpose:** Check the server version, default database, and authenticated account before operating on a target. No placeholders need replacement.

```sql
SELECT VERSION() AS server_version,
       DATABASE() AS current_database,
       CURRENT_USER() AS authenticated_account;
```

**Result:** One context row. GreatSQL reports its own version, for example `8.4.4-5`. A NULL `current_database` means that no default database is selected.

**Official documentation:** [GreatSQL documentation](https://greatsql.cn/docs/)

## 2. List visible databases

**Purpose:** Discover database names visible to the connected account. No placeholders need replacement.

```sql
SHOW DATABASES;
```

**Result:** One database name per row. Permissions can restrict this list.

**Official documentation:** [GreatSQL documentation](https://greatsql.cn/docs/)

## 3. List tables and views in a database

**Purpose:** Discover queryable objects. Replace `database_name` with the target database.

```sql
SHOW FULL TABLES FROM `database_name`;
```

**Result:** Two columns per row: the object name and `BASE TABLE` or `VIEW`. `SHOW TABLES FROM database_name` returns names only.

**Official documentation:** [GreatSQL documentation](https://greatsql.cn/docs/)

## 4. Read a table's CREATE statement

**Purpose:** Obtain the exact DDL used to recreate the table, including its storage engine, character set, and partitioning. Replace the identifiers.

```sql
SHOW CREATE TABLE `database_name`.`table_name`;
```

**Result:** One row with the table name and its CREATE TABLE statement. GreatSQL adds its own storage-engine options where they differ from MySQL.

**Official documentation:** [GreatSQL documentation](https://greatsql.cn/docs/)

## 5. Inspect columns and comments

**Purpose:** Inspect a table's column types, nullability, keys, defaults, and comments. Replace the identifiers.

```sql
SHOW FULL COLUMNS FROM `database_name`.`table_name`;
```

**Result:** One row per column with `Field`, `Type`, `Collation`, `Null`, `Key`, `Default`, `Extra`, `Privileges`, and `Comment`.

**Official documentation:** [GreatSQL documentation](https://greatsql.cn/docs/)

## 6. Inspect indexes and column order

**Purpose:** List the indexes of a table and the column order inside each index. Replace the identifiers.

```sql
SHOW INDEX FROM `database_name`.`table_name`;
```

**Result:** One row per index column with `Key_name`, `Seq_in_index`, `Column_name`, `Collation`, `Cardinality`, and `Index_type`. A composite index appears as one row per column.

**Official documentation:** [GreatSQL documentation](https://greatsql.cn/docs/)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the identifiers.

```sql
SELECT * FROM `table_name` LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real key column when a repeatable sample matters.

**Official documentation:** [GreatSQL documentation](https://greatsql.cn/docs/)

## 8. Read a view or routine definition

**Purpose:** Obtain the stored definition of a view, procedure, or function. Replace the identifiers.

```sql
SHOW CREATE VIEW `database_name`.`view_name`;
SHOW CREATE PROCEDURE `database_name`.`procedure_name`;
```

**Result:** The definition plus its character-set and SQL-mode context. Semicolons inside a routine body belong to that statement; submit the complete definition in one `--command` argument.

**Official documentation:** [GreatSQL documentation](https://greatsql.cn/docs/)

GreatSQL follows MySQL semantics: each `--command` argument is one statement, DDL commonly commits implicitly so a later error does not roll back earlier statements, and `GO` or `DELIMITER` are client directives rather than SQL. Because GreatSQL adds features beyond MySQL, check the documentation of the connected version before relying on a GreatSQL-only statement.
