#!/usr/bin/env python3
"""Exercise every additional database through the CLI against the compose fixtures.

Presto, Hive, Apache Kylin, XuguDB, Db2 and Informix run from tests/compose.yaml after
scripts/jdbc-fixture.sh stages their driver. GBase 8s, Informix, SUNDB and XuguDB depend on assets the
repository cannot fetch, so each fixture reads its own environment variables:

* GBase 8s needs a vendor driver and a running instance: SQLX_TEST_GBASE8S_DRIVER (the jar the vendor
  ships, which may wrap the real ifxjdbc.jar), SQLX_TEST_GBASE8S_PORT, SQLX_TEST_GBASE8S_SERVER and
  SQLX_TEST_GBASE8S_PASSWORD. Driver 3.70.1.61 reads a VARCHAR column back empty; use LVARCHAR.
* Informix uses the developer image's documented default password unless SQLX_TEST_INFORMIX_PASSWORD
  says otherwise; its other identity attempts all answer "is not known on the database server".
* SUNDB needs a licensed installation, since the public vendor image's license expired in 2022:
  SQLX_TEST_SUNDB_PORT and SQLX_TEST_SUNDB_PASSWORD.
* XuguDB ships a trial image whose SYSDBA password is not published, and the driver has no trust mode:
  SQLX_TEST_XUGU_PASSWORD.
"""
import argparse, json, os, subprocess, sys, tempfile, time
from decimal import Decimal
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import contract

ROOT = Path(__file__).resolve().parents[1]
PASSWORD = "sqlx_test_only_password"
# openGauss and OceanBase reject a password without upper case, lower case, a digit and a symbol.
COMPLEX_PASSWORD = "SQLX@TestOnly12345"
QUALIFIED = "sqlx_test.sqlx_values"
FIXTURES = {
    "mariadb": {"port": 23307, "database": "sqlx_test", "username": "root", "password": PASSWORD},
    "cockroachdb": {"port": 26257, "database": "defaultdb", "username": "root", "password": PASSWORD},
    "clickhouse": {"port": 28123, "database": "default", "username": "sqlx", "password": PASSWORD},
    # Trino only accepts a password over TLS, so the plain fixture authenticates by user alone.
    "trino": {"port": 28082, "database": "tpch.tiny", "username": "sqlx", "password": ""},
    "tidb": {"port": 24000, "database": "test", "username": "root", "password": ""},
    # StarRocks and Doris have no default user database, so they connect to a system schema and
    # create their own; TiDB and YugabyteDB ship one the fixture can use directly.
    "starrocks": {"port": 29030, "database": "information_schema", "username": "root", "password": ""},
    "doris": {"port": 29031, "database": "information_schema", "username": "root", "password": ""},
    "yugabytedb": {"port": 25433, "database": "yugabyte", "username": "yugabyte", "password": ""},
    "greatsql": {"port": 23308, "database": "sqlx_test", "username": "root", "password": PASSWORD},
    # openGauss takes the PostgreSQL worker, so it authenticates like the PostgreSQL fixture.
    "opengauss": {"port": 24320, "database": "postgres", "username": "gaussdb", "password": COMPLEX_PASSWORD},
    "oceanbase": {"port": 28811, "database": "oceanbase", "username": "root@sys", "password": COMPLEX_PASSWORD},
    # Dameng and KingbaseES are commercial engines; these fixtures expect a local instance.
    "dameng": {"port": 5236, "database": "", "username": "SYSDBA", "password": os.environ.get("SQLX_TEST_DAMENG_PASSWORD", "SYSDBA_dm001")},
    "kingbase": {"port": 54321, "database": "kingbase", "username": "system", "password": os.environ.get("SQLX_TEST_KINGBASE_PASSWORD", "12345678ab")},
    # TDengine starts without the scratch database, so connect to its catalog and create one.
    "tdengine": {"port": 26041, "database": "information_schema", "username": "root", "password": "taosdata"},
    # SQLite, DuckDB and H2 open a local file, so their fixture is a path inside the test directory.
    "sqlite": {"local": True},
    "duckdb": {"local": True},
    "h2": {"local": True},
    # Presto and Hive need a user but no password; Kylin authenticates with its default ADMIN user.
    "presto": {"port": 28083, "database": "memory.default", "username": "sqlx", "password": ""},
    "hive": {"port": 21000, "database": "default", "username": "hive", "password": ""},
    "kylin": {"port": 37070, "database": "learn_kylin", "username": "ADMIN", "password": "KYLIN"},
    "xugu": {"port": 25138, "database": "SYSTEM", "username": "SYSDBA",
             "password": os.environ.get("SQLX_TEST_XUGU_PASSWORD", "SYSDBA")},
    # Informix and GBase 8s name a server instance, which the connection carries as --service.
    # The developer image keeps its documented default password; DB_INFORMIX_PASSWORD is ignored.
    "informix": {"port": 29088, "database": "sysmaster", "service": "informix",
                 "username": "informix", "password": os.environ.get("SQLX_TEST_INFORMIX_PASSWORD", "in4mix")},
    "gbase8s": {"port": os.environ.get("SQLX_TEST_GBASE8S_PORT", 19088), "database": "gbasedbt",
                "service": os.environ.get("SQLX_TEST_GBASE8S_SERVER", "gbase01"),
                "username": "gbasedbt", "password": os.environ.get("SQLX_TEST_GBASE8S_PASSWORD", "GBase1234")},
    # Db2 is a normal fixture; SUNDB needs a licensed installation, so both expect their instance.
    "db2": {"port": 25000, "database": "sqlxtest", "username": "db2inst1", "password": PASSWORD},
    "sundb": {"port": os.environ.get("SQLX_TEST_SUNDB_PORT", 22581), "database": "goldilocks",
              "username": "sys", "password": os.environ.get("SQLX_TEST_SUNDB_PASSWORD", "gliese")},
}
# Engines that open a local file instead of a server, and the file extension they use.
# H2 appends its own .mv.db suffix to the file name it is given.
LOCAL = {"sqlite": ".db", "duckdb": ".duckdb", "h2": ""}
# Statements that must run before the table batch, for engines without a scratch database.
PREPARE = {
    "starrocks": "CREATE DATABASE IF NOT EXISTS sqlx_test",
    "doris": "CREATE DATABASE IF NOT EXISTS sqlx_test",
    "tdengine": "CREATE DATABASE IF NOT EXISTS sqlx_probe",
}
# Executing DROP TABLE on a missing table is an error for these engines, which have no IF EXISTS form.
TOLERANT_DROP = {"informix", "gbase8s"}
# Kylin answers SQL over pre-built cubes: it takes SELECT but no DDL or DML, and the tables a project
# exposes depend on its cubes, so this fixture verifies the connection and two queries it can always
# answer. The cube tables a deployment serves are documented in references/kylin.md instead.
READ_ONLY = {"kylin": "SELECT 1 + 1 AS two"}
# Engines that answer a query before a storage backend can serve DDL. Wait on an idempotent write,
# not on a status column: an OLAP frontend reports a live backend, and even accepts `CREATE TABLE`,
# before that backend can allocate the table's tablets, and only the insert tells those apart. Every
# attempt starts by dropping the probe table, so a half-finished attempt can always be repeated.
BACKENDS = {
    "starrocks": [
        "DROP TABLE IF EXISTS sqlx_test.sqlx_ready",
        "CREATE TABLE sqlx_test.sqlx_ready (id BIGINT)",
        "INSERT INTO sqlx_test.sqlx_ready VALUES (1)",
    ],
    "doris": [
        "DROP TABLE IF EXISTS sqlx_test.sqlx_ready",
        "CREATE TABLE sqlx_test.sqlx_ready (id BIGINT)",
        "INSERT INTO sqlx_test.sqlx_ready VALUES (1)",
    ],
}
BACKEND_CLEANUP = {
    "starrocks": "DROP TABLE IF EXISTS sqlx_test.sqlx_ready",
    "doris": "DROP TABLE IF EXISTS sqlx_test.sqlx_ready",
}
# TDengine, H2, Presto, Db2 and GBase 8s reserve "value", so their probes alias it differently.
ALIASES = {"tdengine": "ok", "h2": "ok", "presto": "ok", "db2": "ok", "gbase8s": "ok", "kylin": "ok"}


# Db2, Informix and GBase 8s have no FROM-less SELECT, so their probe reads a catalog table.
SELECT_ONE = {
    "db2": "SELECT {value} AS {alias} FROM SYSIBM.SYSDUMMY1",
    "informix": "SELECT {value} AS {alias} FROM systables WHERE tabid = 1",
    "gbase8s": "SELECT {value} AS {alias} FROM systables WHERE tabid = 1",
}


def alias(kind):
    return ALIASES.get(kind, "value")


def select_one(kind, value=1):
    """The one-row probe every engine answers, spelled the way that engine accepts it."""
    template = SELECT_ONE.get(kind, "SELECT {value} AS {alias}")
    return template.format(value=value, alias=alias(kind))


DROP_IF_EXISTS = {
    "mariadb": "DROP TABLE IF EXISTS sqlx_values",
    "cockroachdb": "DROP TABLE IF EXISTS sqlx_values",
    "clickhouse": "DROP TABLE IF EXISTS sqlx_values",
    "trino": "DROP TABLE IF EXISTS memory.default.sqlx_values",
    "tidb": "DROP TABLE IF EXISTS sqlx_values",
    "starrocks": f"DROP TABLE IF EXISTS {QUALIFIED}",
    "doris": f"DROP TABLE IF EXISTS {QUALIFIED}",
    "yugabytedb": "DROP TABLE IF EXISTS sqlx_values",
    "greatsql": "DROP TABLE IF EXISTS sqlx_values",
    "opengauss": "DROP TABLE IF EXISTS sqlx_values",
    "oceanbase": "DROP TABLE IF EXISTS sqlx_values",
    "tdengine": "DROP TABLE IF EXISTS sqlx_probe.recipe_values",
    "dameng": "DROP TABLE IF EXISTS sqlx_values",
    "kingbase": "DROP TABLE IF EXISTS sqlx_values",
    "sqlite": "DROP TABLE IF EXISTS sqlx_values",
    "duckdb": "DROP TABLE IF EXISTS sqlx_values",
    "h2": "DROP TABLE IF EXISTS sqlx_values",
    # Presto, Hive, Kylin and XuguDB keep the table in the schema --database selects.
    "presto": "DROP TABLE IF EXISTS memory.default.sqlx_values",
    "hive": "DROP TABLE IF EXISTS sqlx_values",
    "xugu": "DROP TABLE IF EXISTS sqlx_values",
    "db2": "DROP TABLE IF EXISTS sqlx_values",
    "informix": "DROP TABLE sqlx_values",
    "sundb": "DROP TABLE IF EXISTS sqlx_values",
    "gbase8s": "DROP TABLE sqlx_values",
}
CREATE = {
    "mariadb": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "cockroachdb": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "clickhouse": "CREATE TABLE sqlx_values (id Int64, amount Decimal(18,4), label String) ENGINE = Memory",
    "trino": "CREATE TABLE memory.default.sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "tidb": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "starrocks": f"CREATE TABLE {QUALIFIED} (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "doris": f"CREATE TABLE {QUALIFIED} (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "yugabytedb": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "greatsql": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "opengauss": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "oceanbase": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "tdengine": "CREATE TABLE sqlx_probe.recipe_values (ts TIMESTAMP, id BIGINT, amount DECIMAL(30,4), label NCHAR(100))",
    "dameng": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "kingbase": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "sqlite": "CREATE TABLE sqlx_values (id INTEGER, amount NUMERIC, label TEXT)",
    "duckdb": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "h2": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    # PrestoDB persists tables through a connector, so the fixture uses the memory connector.
    "presto": "CREATE TABLE memory.default.sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    # Hive has no DECIMAL(30,4) default here, but it accepts the ANSI spelling.
    "hive": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "xugu": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "db2": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    # Informix and GBase 8s have no BIGINT, so the wide integer uses DECIMAL(20,0).
    "informix": "CREATE TABLE sqlx_values (id DECIMAL(20,0), amount DECIMAL(30,4), label VARCHAR(100))",
    # The GBase 8s driver 3.70.1.61 reads a VARCHAR column back empty; LVARCHAR is the varying type
    # its own documentation recommends, and it round-trips.
    "gbase8s": "CREATE TABLE sqlx_values (id DECIMAL(20,0), amount DECIMAL(30,4), label LVARCHAR(100))",
    "sundb": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
}
INSERT = {
    "mariadb": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "cockroachdb": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "clickhouse": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "trino": "INSERT INTO memory.default.sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "tidb": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "starrocks": f"INSERT INTO {QUALIFIED} VALUES (9007199254740993, 123.4500, 'hello')",
    "doris": f"INSERT INTO {QUALIFIED} VALUES (9007199254740993, 123.4500, 'hello')",
    "yugabytedb": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "greatsql": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "opengauss": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "oceanbase": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "tdengine": "INSERT INTO sqlx_probe.recipe_values VALUES (NOW, 9007199254740993, 123.4500, 'hello')",
    "dameng": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "kingbase": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "sqlite": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "duckdb": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "h2": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "presto": "INSERT INTO memory.default.sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "hive": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "xugu": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "db2": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "informix": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "gbase8s": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "sundb": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
}
SELECT = {
    "mariadb": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "cockroachdb": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    # The ClickHouse JDBC driver rejects result sets whose columns share a label.
    "clickhouse": "SELECT id, amount, label FROM sqlx_values",
    "trino": "SELECT id AS DUP, id AS DUP, amount, label FROM memory.default.sqlx_values",
    "tidb": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "starrocks": f"SELECT id AS DUP, id AS DUP, amount, label FROM {QUALIFIED}",
    "doris": f"SELECT id AS DUP, id AS DUP, amount, label FROM {QUALIFIED}",
    "yugabytedb": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "greatsql": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "opengauss": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "oceanbase": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "tdengine": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_probe.recipe_values",
    "dameng": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "kingbase": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "sqlite": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "duckdb": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "h2": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "presto": "SELECT id AS DUP, id AS DUP, amount, label FROM memory.default.sqlx_values",
    # HiveServer2 renames a repeated column label instead of returning both under one name.
    "hive": "SELECT id, amount, label FROM sqlx_values",
    "xugu": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "db2": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    # The Informix-derived drivers reject a result set whose columns share a label.
    "informix": "SELECT id, amount, label FROM sqlx_values",
    "gbase8s": "SELECT id, amount, label FROM sqlx_values",
    "sundb": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
}


def retry(kind, description, action, attempts=6, delay=5):
    """Repeat an action that cannot write twice: a loaded fixture may fail a statement once."""
    for attempt in range(attempts):
        try:
            return action()
        except AssertionError as error:
            if attempt == attempts - 1:
                raise
            print(f"{kind}: retrying {description} after: {str(error)[:160]}", flush=True)
            time.sleep(delay)


def exercise(cli, bin_dir, kind):
    fixture = FIXTURES[kind]
    with tempfile.TemporaryDirectory(prefix=f"sqlx-{kind}-test-") as tmp:
        if fixture.get("local"):
            connection = dict(database_type=kind, host="", port=0, database=str(Path(tmp)/f"fixture{LOCAL[kind]}"),
                              service="", username="", password="", tls="disable")
        else:
            connection = dict(database_type=kind, host="127.0.0.1", port=fixture["port"], database=fixture["database"],
                              service=fixture.get("service", ""), username=fixture["username"],
                              password=fixture["password"], tls="disable")
        def call(*args, ok=True, payload=None):
            result = subprocess.run([str(cli), "--data-dir", tmp, "--worker-dir", str(bin_dir), *args],
                                    input=json.dumps(payload) if payload else None, text=True, capture_output=True,
                                    timeout=120, env=os.environ.copy())
            value = json.loads(result.stdout)
            if ok:
                assert result.returncode == 0 and value["success"], value
            return result.returncode, value

        call("datasource", "add", "--name", "fixture", "--connection-stdin", payload=connection)
        # A heavy engine can take minutes to accept the first connection.
        for attempt in range(1 if fixture.get("local") else 60):
            code, result = call("datasource", "test", "--id", "fixture", ok=False)
            if code == 0:
                break
            if attempt == 59:
                raise AssertionError(result)
            time.sleep(3)
        # A reachable endpoint can still refuse queries while the engine registers its worker.
        for attempt in range(1 if fixture.get("local") else 40):
            code, result = call("sql", "execute", "--datasource", "fixture",
                                "--command", select_one(kind), ok=False)
            if code == 0:
                break
            if attempt == 39:
                raise AssertionError(result)
            time.sleep(3)
        # Engines without a scratch database need theirs before the readiness probe below.
        if kind in PREPARE:
            call("sql", "execute", "--datasource", "fixture", "--command", PREPARE[kind])
        # An OLAP frontend accepts queries, reports a live backend and even creates the table before
        # that backend can allocate its tablets, so the probe writes a row. It starts by dropping the
        # probe table, which keeps every retry repeatable, and the write batch below still runs once.
        if kind in BACKENDS:
            for attempt in range(60):
                for statement in BACKENDS[kind]:
                    code, result = call("sql", "execute", "--datasource", "fixture", "--command", statement, ok=False)
                    if code != 0:
                        break
                if code == 0:
                    call("sql", "execute", "--datasource", "fixture", "--command", BACKEND_CLEANUP[kind], ok=False)
                    break
                if attempt == 0:
                    print(f"{kind}: waiting for a storage backend that can hold a table", flush=True)
                if attempt == 59:
                    raise AssertionError(result)
                time.sleep(5)
        # A read-only engine is verified by a query it can always answer and by the first-error stop
        # below; a cube query needs the deployment's own project, which the fixture cannot know.
        if kind in READ_ONLY:
            _, result = retry(kind, "the read-only query", lambda: call(
                "sql", "execute", "--datasource", "fixture", "--command", READ_ONLY[kind]))
            rows = contract.rows(result)
            assert len(rows) == 1, result
            assert rows[0][0] == "2", result
            assert contract.columns(result)[0][0].lower() == "two", result

            def read_only_error_batch():
                code, result = call("sql", "execute", "--datasource", "fixture",
                                    "--command", select_one(kind),
                                    "--command", "SELECT * FROM missing_table",
                                    "--command", select_one(kind, 2), ok=False)
                error = contract.error(result, 1)
                assert error is not None and contract.rows(result, 0), result
                assert code != 0 and contract.skipped(result) == [2], result
            retry(kind, "the first-error batch", read_only_error_batch)
            print(f"{kind}: connection, cube query and first-error stop passed (read-only engine)")
            return
        # Writes are submitted once: replaying this batch could apply them twice. An engine without
        # DROP TABLE IF EXISTS gets its drop first, where a missing table is allowed to fail.
        writes = [DROP_IF_EXISTS[kind], CREATE[kind], INSERT[kind]]
        if kind in TOLERANT_DROP:
            dropped = call("sql", "execute", "--datasource", "fixture",
                           "--command", DROP_IF_EXISTS[kind], ok=False)
            writes = writes[1:]
        call("sql", "execute", "--datasource", "fixture", *[arg for statement in writes for arg in ("--command", statement)])
        _, result = retry(kind, "the read-only query", lambda: call(
            "sql", "execute", "--datasource", "fixture", "--command", SELECT[kind]))
        rows = contract.rows(result)
        assert len(rows) == 1, result
        row = rows[0]
        if kind in ("clickhouse", "hive", "informix", "gbase8s"):
            assert row == ["9007199254740993", "123.4500", "hello"], row
        else:
            assert row[:2] == ["9007199254740993", "9007199254740993"], row
            assert Decimal(row[2]) == Decimal("123.4500"), row
            assert row[3] == "hello", row
            columns = contract.columns(result)
            assert columns[0][0] == columns[1][0], columns
            assert columns[0][0].lower() == "dup", columns
        def first_error_batch():
            code, result = call("sql", "execute", "--datasource", "fixture",
                                "--command", select_one(kind),
                                "--command", "SELECT * FROM sqlx_missing_table",
                                "--command", select_one(kind, 2), ok=False)
            error = contract.error(result, 1)
            assert error is not None and contract.rows(result, 0), result
            assert code != 0 and contract.skipped(result) == [2], result
        retry(kind, "the first-error batch", first_error_batch)
        # The cleanup is idempotent, so an interrupted drop can be repeated safely.
        retry(kind, "the idempotent cleanup", lambda: call(
            "sql", "execute", "--datasource", "fixture", "--command", DROP_IF_EXISTS[kind],
            ok=kind not in TOLERANT_DROP))
        print(f"{kind}: connection, DDL/DML/query, numeric precision, duplicate columns and first-error stop passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("kinds", nargs="*", help=f"subset of {', '.join(sorted(FIXTURES))}; default all")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug" / ("sqlx.exe" if os.name == "nt" else "sqlx"))
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/debug")
    args = parser.parse_args()
    kinds = args.kinds or sorted(FIXTURES)
    unknown = [kind for kind in kinds if kind not in FIXTURES]
    if unknown:
        parser.error(f"unknown database {unknown[0]}; expected {', '.join(sorted(FIXTURES))}")
    for kind in kinds:
        exercise(args.cli, args.bin_dir, kind)
