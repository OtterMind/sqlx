#!/usr/bin/env python3
"""Exercise MariaDB, CockroachDB, ClickHouse and Trino through the CLI against the compose fixtures."""
import argparse, json, os, subprocess, tempfile, time
from decimal import Decimal
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PASSWORD = "sqlx_test_only_password"
FIXTURES = {
    "mariadb": {"port": 23307, "database": "sqlx_test", "username": "root", "password": PASSWORD},
    "cockroachdb": {"port": 26257, "database": "defaultdb", "username": "root", "password": PASSWORD},
    "clickhouse": {"port": 28123, "database": "default", "username": "sqlx", "password": PASSWORD},
    # Trino only accepts a password over TLS, so the plain fixture authenticates by user alone.
    "trino": {"port": 28082, "database": "tpch.tiny", "username": "sqlx", "password": ""},
}
DROP_IF_EXISTS = {
    "mariadb": "DROP TABLE IF EXISTS sqlx_values",
    "cockroachdb": "DROP TABLE IF EXISTS sqlx_values",
    "clickhouse": "DROP TABLE IF EXISTS sqlx_values",
    "trino": "DROP TABLE IF EXISTS memory.default.sqlx_values",
}
CREATE = {
    "mariadb": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "cockroachdb": "CREATE TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
    "clickhouse": "CREATE TABLE sqlx_values (id Int64, amount Decimal(18,4), label String) ENGINE = Memory",
    "trino": "CREATE TABLE memory.default.sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))",
}
INSERT = {
    "mariadb": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "cockroachdb": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "clickhouse": "INSERT INTO sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
    "trino": "INSERT INTO memory.default.sqlx_values VALUES (9007199254740993, 123.4500, 'hello')",
}
SELECT = {
    "mariadb": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    "cockroachdb": "SELECT id AS DUP, id AS DUP, amount, label FROM sqlx_values",
    # The ClickHouse JDBC driver rejects result sets whose columns share a label.
    "clickhouse": "SELECT id, amount, label FROM sqlx_values",
    "trino": "SELECT id AS DUP, id AS DUP, amount, label FROM memory.default.sqlx_values",
}
DROP = {
    "mariadb": "DROP TABLE sqlx_values",
    "cockroachdb": "DROP TABLE sqlx_values",
    "clickhouse": "DROP TABLE sqlx_values",
    "trino": "DROP TABLE memory.default.sqlx_values",
}


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
        args = ["sql", "execute", "--datasource", "fixture"]
        for statement in (DROP_IF_EXISTS[kind], CREATE[kind], INSERT[kind], SELECT[kind]):
            args += ["--sql", statement]
        _, result = call(*args)
        row = next(e for e in result["events"] if e["event"] == "row" and e["index"] == 3)
        if kind == "clickhouse":
            assert row["values"] == ["9007199254740993", "123.4500", "hello"], row
        else:
            assert row["values"][:2] == ["9007199254740993", "9007199254740993"], row
            assert Decimal(row["values"][2]) == Decimal("123.4500"), row
            assert row["values"][3] == "hello", row
            columns = next(e for e in result["events"] if e["event"] == "columns" and e["index"] == 3)
            assert columns["columns"][0]["name"] == columns["columns"][1]["name"], columns
            assert columns["columns"][0]["name"].lower() == "dup", columns
        code, result = call("sql", "execute", "--datasource", "fixture", "--sql", "SELECT 1 AS value",
                            "--sql", "SELECT * FROM sqlx_missing_table", "--sql", "SELECT 2 AS value", ok=False)
        assert code != 0 and any(e["event"] == "skipped" and e["index"] == 2 for e in result["events"]), result
        call("sql", "execute", "--datasource", "fixture", "--sql", DROP[kind])
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
