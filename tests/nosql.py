#!/usr/bin/env python3
"""Exercise the non-SQL workers (Redis today, MongoDB next) through the CLI against compose fixtures."""
import argparse, json, os, subprocess, tempfile, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PASSWORD = "sqlx_test_only_password"
FIXTURES = {
    "redis": {"port": 26379, "database": "0", "username": "", "password": PASSWORD},
    "mongodb": {"port": 27018, "database": "sqlx_probe", "username": "root", "password": PASSWORD},
}


def connection_for(kind):
    fixture = FIXTURES[kind]
    connection = dict(database_type=kind, host="127.0.0.1", port=fixture["port"], database=fixture["database"],
                      service="", username=fixture["username"], password=fixture["password"], tls="disable")
    if kind == "mongodb":
        connection["properties"] = {"authSource": "admin"}
    return connection


def exercise_redis(cli, bin_dir, tmp, call):
    call("sql", "execute", "--datasource", "fixture", "--command", "FLUSHDB")
    _, result = call("sql", "execute", "--datasource", "fixture",
                     "--command", 'SET greeting "hello world"', "--command", "GET greeting",
                     "--command", "DEL greeting")
    values = [event["values"] for event in result["events"] if event["event"] == "row"]
    assert values == [["OK"], ["hello world"], ["1"]], values
    integers = [event for event in result["events"] if event["event"] == "columns" and event["columns"]]
    assert integers[2]["columns"][0]["database_type"] == "integer", integers[2]

    _, result = call("sql", "execute", "--datasource", "fixture",
                     "--command", "HSET user:1 name ada role engineer", "--command", "HGETALL user:1")
    columns = [event["columns"] for event in result["events"] if event["event"] == "columns" and event["columns"]]
    assert [column["name"] for column in columns[1]] == ["field", "value"], columns[1]
    pairs = [event["values"] for event in result["events"] if event["event"] == "row"]
    assert pairs[1:] == [["name", "ada"], ["role", "engineer"]], pairs

    _, result = call("sql", "execute", "--datasource", "fixture",
                     "--command", "RPUSH queue a b c", "--command", "LRANGE queue 0 -1")
    rows = [event["values"] for event in result["events"] if event["event"] == "row"]
    assert rows[1:] == [["a"], ["b"], ["c"]], rows

    _, result = call("sql", "execute", "--datasource", "fixture", "--command", "SCAN 0 COUNT 10")
    scan = [event["values"] for event in result["events"] if event["event"] == "row"]
    assert len(scan) == 1 and scan[0][0].isdigit() and scan[0][1].startswith("["), scan
    columns = [event["columns"] for event in result["events"] if event["event"] == "columns" and event["columns"]]
    assert [column["name"] for column in columns[0]] == ["c1", "c2"], columns[0]

    code, result = call("sql", "execute", "--datasource", "fixture",
                        "--command", "SET plain value", "--command", "HGETALL plain",
                        "--command", "GET plain", ok=False)
    error = next(event for event in result["events"] if event["event"] == "error")
    assert code != 0 and error["code"] == "redis.WRONGTYPE" and error["outcome"] == "failed", error
    assert any(event["event"] == "skipped" and event["index"] == 2 for event in result["events"]), result

    binary_seeded = seed_binary_key()
    if binary_seeded:
        _, result = call("sql", "execute", "--datasource", "fixture", "--command", "GET binary:key")
        columns = [event["columns"] for event in result["events"] if event["event"] == "columns" and event["columns"]]
        row = next(event["values"] for event in result["events"] if event["event"] == "row")
        assert columns[0][0]["encoding"] == "base64", columns[0]
        assert row == ["/wD+"], row
    else:
        print("redis: binary reply check skipped, no docker access to seed the key")
    call("sql", "execute", "--datasource", "fixture", "--command", "FLUSHDB")
    print("redis: connection, write/read, pair and array replies, nested reply, "
          "server error mapping and first-error stop passed")


def seed_binary_key():
    """Store bytes that are not valid UTF-8, so the reply has to be base64."""
    probe = ["docker", "exec", "sqlx-integration-redis-1", "sh", "-c",
             "printf '\\xff\\x00\\xfe' | redis-cli -a " + PASSWORD + " -x SET binary:key"]
    try:
        result = subprocess.run(probe, text=True, capture_output=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        return False
    return result.returncode == 0


def exercise(cli, bin_dir, kind):
    with tempfile.TemporaryDirectory(prefix=f"sqlx-{kind}-test-") as tmp:
        def call(*args, ok=True, payload=None):
            result = subprocess.run([str(cli), "--data-dir", tmp, "--worker-dir", str(bin_dir), *args],
                                    input=json.dumps(payload) if payload else None, text=True, capture_output=True,
                                    timeout=120, env=os.environ.copy())
            value = json.loads(result.stdout)
            if ok:
                assert result.returncode == 0 and value["success"], value
            return result.returncode, value

        call("datasource", "add", "--name", "fixture", "--connection-stdin", payload=connection_for(kind))
        for attempt in range(40):
            code, result = call("datasource", "test", "--id", "fixture", ok=False)
            if code == 0:
                break
            if attempt == 39:
                raise AssertionError(result)
            time.sleep(2)
        if kind == "redis":
            exercise_redis(cli, bin_dir, tmp, call)
        else:
            raise AssertionError(f"no checks written for {kind} yet")


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
