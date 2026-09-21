# MySQL database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `database_name`, `table_name`, `view_name`, and `procedure_name` with actual identifiers. Quote identifiers with backticks and double embedded backticks. Single-quoted SQL strings have separate escaping rules.

Official links target MySQL 8.4. The documentation root is https://dev.mysql.com/doc/refman/8.4/en/. Match the connected server version when you look them up.

## 1. Identify the current connection

**Purpose:** Check the server version, default database, and authenticated database account before operating on a target. No placeholders need replacement.

```sql
SELECT VERSION() AS server_version,
       DATABASE() AS current_database,
       CURRENT_USER() AS authenticated_account;
```

**Result:** One context row. A NULL `current_database` means that no default database is selected.

**Official documentation:** [Information functions](https://dev.mysql.com/doc/refman/8.4/en/information-functions.html)

## 2. List visible databases

**Purpose:** Discover database names visible to the connected account. No placeholders need replacement.

```sql
SHOW DATABASES;
```

**Result:** One database name per row. Permissions can restrict this list; it need not contain every database on the server.

**Official documentation:** [SHOW DATABASES](https://dev.mysql.com/doc/refman/8.4/en/show-databases.html)

## 3. List tables and views in a database

**Purpose:** Discover queryable objects. Replace `database_name` with the target database.

```sql
SHOW FULL TABLES FROM `database_name`;
```

**Result:** Object names and types. `BASE TABLE` identifies a table; `VIEW` identifies a view.

**Official documentation:** [SHOW TABLES](https://dev.mysql.com/doc/refman/8.4/en/show-tables.html)

## 4. Read a table's CREATE statement

**Purpose:** Obtain server-generated table DDL. Replace both the database and table identifiers.

```sql
SHOW CREATE TABLE `database_name`.`table_name`;
```

**Result:** The table name and CREATE TABLE statement, normally including columns, keys, indexes, and table options. This is not a whole-schema export: data, triggers, and other dependent objects require separate handling.

**Official documentation:** [SHOW CREATE TABLE](https://dev.mysql.com/doc/refman/8.4/en/show-create-table.html)

## 5. Inspect columns and comments

**Purpose:** Inspect types, nullability, defaults, collation, extra attributes, and comments. Replace the database and table identifiers.

```sql
SHOW FULL COLUMNS FROM `database_name`.`table_name`;
```

**Result:** One row per column. Read `Field`, `Type`, `Null`, `Default`, `Extra`, and `Comment`; for example, `Extra` indicates an auto-increment column.

**Official documentation:** [SHOW COLUMNS](https://dev.mysql.com/doc/refman/8.4/en/show-columns.html)

## 6. Inspect indexes and column order

**Purpose:** Identify indexes, uniqueness, and the order of columns in composite indexes. Replace the database and table identifiers.

```sql
SHOW INDEX FROM `database_name`.`table_name`;
```

**Result:** One row per indexed column. `Key_name` names the index, `Non_unique=0` means unique, and `Seq_in_index` gives the column position. Several rows can belong to one index.

**Official documentation:** [SHOW INDEX](https://dev.mysql.com/doc/refman/8.4/en/show-index.html)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the database and table identifiers.

```sql
SELECT * FROM `database_name`.`table_name` LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real primary key when a repeatable sample matters.

**Official documentation:** [SELECT and LIMIT](https://dev.mysql.com/doc/refman/8.4/en/select.html)

## 8. Read a view definition

**Purpose:** Obtain CREATE VIEW for a named view. Replace the database and view identifiers.

```sql
SHOW CREATE VIEW `database_name`.`view_name`;
```

**Result:** The view definition and associated character-set context. Definition access depends on privileges.

**Official documentation:** [SHOW CREATE VIEW](https://dev.mysql.com/doc/refman/8.4/en/show-create-view.html)

## 9. Read a stored procedure definition

**Purpose:** Obtain the creation SQL for a procedure. Replace the database and procedure identifiers.

```sql
SHOW CREATE PROCEDURE `database_name`.`procedure_name`;
```

**Result:** Procedure definition, SQL mode, and character-set context. Semicolons inside a routine body belong to that statement. Submit the complete definition in one `--sql` argument; do not submit the client-only `DELIMITER` directive.

**Official documentation:** [SHOW CREATE PROCEDURE](https://dev.mysql.com/doc/refman/8.4/en/show-create-procedure.html)

## 10. Select a default database for this invocation

**Purpose:** Change the default database for subsequent statements in the same CLI invocation. Replace the database identifier.

```sql
USE `database_name`;
```

**Result:** Execution status, not table data. This state does not survive another CLI invocation. Qualified object names or a datasource with the intended default database are usually easier to use.

**Official documentation:** [USE](https://dev.mysql.com/doc/refman/8.4/en/use.html) · [Implicit commits](https://dev.mysql.com/doc/refman/8.4/en/implicit-commit.html)

Many MySQL DDL statements implicitly commit. A later batch error does not roll back earlier autocommitted statements. The native worker uses `utf8mb4` for text.
