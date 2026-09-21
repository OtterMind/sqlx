#!/usr/bin/env python3
"""Exercise every additional database through the CLI against the compose fixtures."""
import argparse, json, os, subprocess, tempfile, time
from decimal import Decimal
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PASSWORD = "sqlx_test_only_password"
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
    "opengauss": {"port": 24320, "database": "postgres", "username": "gaussdb", "password": PASSWORD},
    "oceanbase": {"port": 28811, "database": "oceanbase", "username": "root@sys", "password": PASSWORD},
    # The TDengine RESTful driver reaches taosAdapter and has no user password change here.
    "tdengine": {"port": 26041, "database": "sqlx_probe", "username": "root", "password": "taosdata"},
}
# Statements that must run before the table batch, for engines without a scratch database.
PREPARE = {
    "starrocks": "CREATE DATABASE IF NOT EXISTS sqlx_test",
    "doris": "CREATE DATABASE IF NOT EXISTS sqlx_test",
    "tdengine": "CREATE DATABASE IF NOT EXISTS sqlx_probe",
}
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
# TDengine reserves "value", so its readiness and error probes alias the column differently.
ALIASES = {"tdengine": "ok"}


def alias(kind):
    return ALIASES.get(kind, "value")


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
    connection = dict(database_type=kind, host="127.0.0.1", port=fixture["port"], database=fixture["database"],
                      service="", username=fixture["username"], password=fixture["password"], tls="disable")
    with tempfile.TemporaryDirectory(prefix=f"sqlx-{kind}-test-") as tmp:
        def call(*args, ok=True, payload=None):
            result = subprocess.run([str(cli), "--data-dir", tmp, "--worker-dir", str(bin_dir), *args],
                                    input=json.dumps(payload) if payload else None, text=True, capture_output=True,
                                    timeout=120, env=os.environ.copy())
            value = json.loads(result.stdout)
            if ok:
                assert result.returncode == 0 and value["success"], value
            return result.returncode, value

        call("datasource", "add", "--name", "fixture", "--connection-stdin", payload=connection)
        for attempt in range(40):
            code, result = call("datasource", "test", "--id", "fixture", ok=False)
            if code == 0:
                break
            if attempt == 39:
                raise AssertionError(result)
            time.sleep(3)
        # A reachable endpoint can still refuse queries while the engine registers its worker.
        for attempt in range(40):
            code, result = call("sql", "execute", "--datasource", "fixture",
                                "--sql", f"SELECT 1 AS {alias(kind)}", ok=False)
            if code == 0:
                break
            if attempt == 39:
                raise AssertionError(result)
            time.sleep(3)
        # Engines without a scratch database need theirs before the readiness probe below.
        if kind in PREPARE:
            call("sql", "execute", "--datasource", "fixture", "--sql", PREPARE[kind])
        # An OLAP frontend accepts queries, reports a live backend and even creates the table before
        # that backend can allocate its tablets, so the probe writes a row. It starts by dropping the
        # probe table, which keeps every retry repeatable, and the write batch below still runs once.
        if kind in BACKENDS:
            for attempt in range(60):
                for statement in BACKENDS[kind]:
                    code, result = call("sql", "execute", "--datasource", "fixture", "--sql", statement, ok=False)
                    if code != 0:
                        break
                if code == 0:
                    call("sql", "execute", "--datasource", "fixture", "--sql", BACKEND_CLEANUP[kind], ok=False)
                    break
                if attempt == 0:
                    print(f"{kind}: waiting for a storage backend that can hold a table", flush=True)
                if attempt == 59:
                    raise AssertionError(result)
                time.sleep(5)
        # Writes are submitted once: replaying this batch could apply them twice.
        writes = [DROP_IF_EXISTS[kind], CREATE[kind], INSERT[kind]]
        call("sql", "execute", "--datasource", "fixture", *[arg for statement in writes for arg in ("--sql", statement)])
        _, result = retry(kind, "the read-only query", lambda: call(
            "sql", "execute", "--datasource", "fixture", "--sql", SELECT[kind]))
        row = next(e for e in result["events"] if e["event"] == "row" and e["index"] == 0)
        if kind == "clickhouse":
            assert row["values"] == ["9007199254740993", "123.4500", "hello"], row
        else:
            assert row["values"][:2] == ["9007199254740993", "9007199254740993"], row
            assert Decimal(row["values"][2]) == Decimal("123.4500"), row
            assert row["values"][3] == "hello", row
            columns = next(e for e in result["events"] if e["event"] == "columns" and e["index"] == 0)
            assert columns["columns"][0]["name"] == columns["columns"][1]["name"], columns
            assert columns["columns"][0]["name"].lower() == "dup", columns
        def first_error_batch():
            code, result = call("sql", "execute", "--datasource", "fixture",
                                "--sql", f"SELECT 1 AS {alias(kind)}",
                                "--sql", "SELECT * FROM sqlx_missing_table",
                                "--sql", f"SELECT 2 AS {alias(kind)}", ok=False)
            error = next((e for e in result["events"] if e["event"] == "error"), None)
            assert error is not None and error["index"] == 1, result
            assert code != 0 and any(e["event"] == "skipped" and e["index"] == 2 for e in result["events"]), result
        retry(kind, "the first-error batch", first_error_batch)
        # The cleanup is idempotent, so an interrupted drop can be repeated safely.
        retry(kind, "the idempotent cleanup", lambda: call(
            "sql", "execute", "--datasource", "fixture", "--sql", DROP_IF_EXISTS[kind]))
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
