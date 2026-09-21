# MariaDB database operations

Each SQL block below performs one operation. Submit it through `sqlx sql execute --datasource <id> --sql "..."`. Repeat `--sql` in the same invocation when operations need to share connection state.

Replace `database_name`, `table_name`, `view_name`, and `procedure_name` with actual identifiers. Quote identifiers with backticks and double embedded backticks. Single-quoted SQL strings have separate escaping rules.

MariaDB speaks the MySQL protocol, so `--type mariadb` uses the MySQL worker and a `mysql` datasource reaches the same server. Official links target the current MariaDB documentation. Open the page for the version of the connected server, and when a link has moved, search that vendor's site rather than a third-party copy.

## 1. Identify the current connection

**Purpose:** Check the server version, default database, and authenticated account before operating on a target. No placeholders need replacement.

```sql
SELECT VERSION() AS server_version,
       DATABASE() AS current_database,
       CURRENT_USER() AS authenticated_account;
```

**Result:** One context row. A NULL `current_database` means that no default database is selected.

**Official documentation:** [VERSION](https://mariadb.com/kb/en/version/) · [DATABASE](https://mariadb.com/kb/en/database/) · [CURRENT_USER](https://mariadb.com/kb/en/current_user/)

## 2. List visible databases

**Purpose:** Discover database names visible to the connected account. No placeholders need replacement.

```sql
SHOW DATABASES;
```

**Result:** One database name per row. Permissions can restrict this list.

**Official documentation:** [SHOW DATABASES](https://mariadb.com/kb/en/show-databases/)

## 3. List tables and views in a database

**Purpose:** Discover queryable objects. Replace `database_name` with the target database.

```sql
SHOW FULL TABLES FROM `database_name`;
```

**Result:** Object names and types. `BASE TABLE` identifies a table; `VIEW` identifies a view.

**Official documentation:** [SHOW TABLES](https://mariadb.com/kb/en/show-tables/)

## 4. Read a table's CREATE statement

**Purpose:** Obtain server-generated table DDL. Replace both the database and table identifiers.

```sql
SHOW CREATE TABLE `database_name`.`table_name`;
```

**Result:** The table name and CREATE TABLE statement, normally including columns, keys, indexes, and table options. Data, triggers, and other dependent objects require separate handling.

**Official documentation:** [SHOW CREATE TABLE](https://mariadb.com/kb/en/show-create-table/)

## 5. Inspect columns and comments

**Purpose:** Inspect types, nullability, defaults, collation, extra attributes, and comments. Replace the database and table identifiers.

```sql
SHOW FULL COLUMNS FROM `database_name`.`table_name`;
```

**Result:** One row per column. Read `Field`, `Type`, `Null`, `Default`, `Extra`, and `Comment`.

**Official documentation:** [SHOW COLUMNS](https://mariadb.com/kb/en/show-columns/)

## 6. Inspect indexes and column order

**Purpose:** Identify indexes, uniqueness, and the order of columns in composite indexes. Replace the database and table identifiers.

```sql
SHOW INDEX FROM `database_name`.`table_name`;
```

**Result:** One row per indexed column. `Key_name` names the index, `Non_unique=0` means unique, and `Seq_in_index` gives the column position.

**Official documentation:** [SHOW INDEX](https://mariadb.com/kb/en/show-index/)

## 7. Preview up to 100 rows

**Purpose:** Inspect a small sample while controlling the agent's output volume. Replace the database and table identifiers.

```sql
SELECT * FROM `database_name`.`table_name` LIMIT 100;
```

**Result:** At most 100 rows. Without ORDER BY the order is unspecified; add ordering by a real primary key when a repeatable sample matters.

**Official documentation:** [SELECT](https://mariadb.com/kb/en/select/)

## 8. Read a view or procedure definition

**Purpose:** Obtain the stored definition of a view or procedure. Replace the identifiers.

```sql
SHOW CREATE VIEW `database_name`.`view_name`;
SHOW CREATE PROCEDURE `database_name`.`procedure_name`;
```

**Result:** The definition plus its character-set and SQL-mode context. Semicolons inside a routine body belong to that statement; submit the complete definition in one `--sql` argument and never the client-only `DELIMITER` directive.

**Official documentation:** [SHOW CREATE VIEW](https://mariadb.com/kb/en/show-create-view/) · [SHOW CREATE PROCEDURE](https://mariadb.com/kb/en/show-create-procedure/)

MariaDB DDL commonly commits implicitly, so a later batch error does not roll back earlier autocommitted statements. MariaDB adds syntax that MySQL lacks, such as `RETURNING` for some statements and `INSERT ... RETURNING`; check the server version before relying on it.
