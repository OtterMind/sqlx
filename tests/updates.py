#!/usr/bin/env python3
"""Real self-replacement of disposable CLI copies through a local release server."""
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import threading
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]
CURRENT = "0.1.17"
NEXT = "0.1.99"
SUFFIX = ".exe" if os.name == "nt" else ""
NAME = "sqlx" + SUFFIX
CLI = ROOT / "target/debug" / NAME
OS = "windows" if os.name == "nt" else "macos" if platform.system() == "Darwin" else "linux"
ARCH = "arm64" if platform.machine().lower() in ("arm64", "aarch64") else "x64"
PLATFORM = OS + "-" + ARCH


def main():
    with tempfile.TemporaryDirectory(prefix="sqlx-updates-") as directory:
        root = Path(directory)
        fixture_cli = root / NAME
        shutil.copy2(CLI, fixture_cli)
        if OS == "linux":
            # Release binaries are stripped. Hashing 80+ MB of debug symbols with
            # an unoptimized ARM SHA implementation exhausts the test timeout.
            # Strip only the disposable copy; preserve all replacement assertions.
            subprocess.run(["strip", "--strip-debug", str(fixture_cli)], check=True)
        print(f"CLI fixture: {CLI.stat().st_size} -> {fixture_cli.stat().st_size} bytes", flush=True)
        web = root / "web"
        web.mkdir()
        requests = []
        download_started = threading.Event()
        delayed_download = False

        class Handler(http.server.SimpleHTTPRequestHandler):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, directory=str(web), **kwargs)

            def log_message(self, *args):
                pass

            def do_GET(self):
                requests.append(self.path)
                if delayed_download and self.path.endswith(".zip"):
                    download_started.set()
                    time.sleep(1)
                super().do_GET()

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        base = f"http://127.0.0.1:{server.server_port}/releases"

        def compile_candidate(kind):
            source = root / (kind + ".rs")
            binary = root / (kind + SUFFIX)
            extra = ""
            identity = f"sqlx {NEXT} (OtterMind/sqlx)"
            if kind == "wrong":
                identity = "another product 99.0.0"
            if kind == "final_failure":
                extra = 'if std::env::current_exe().unwrap().parent().unwrap().file_name().unwrap() == "installed" { std::process::exit(42); }'
            if kind == "timeout":
                extra = "std::thread::sleep(std::time::Duration::from_secs(20));"
            source.write_text('fn main() {' + extra + 'println!("' + identity + '");}\n')
            subprocess.run(["rustc", "--edition=2021", str(source), "-o", str(binary)], check=True)
            return binary

        good = compile_candidate("good")
        wrong = compile_candidate("wrong")
        final_failure = compile_candidate("final_failure")
        timeout = compile_candidate("timeout")

        def publish(binary=good, version=NEXT, traversal=False):
            release = web / "releases/download" / ("v" + version)
            release.mkdir(parents=True, exist_ok=True)
            archive = release / ("sqlx-" + PLATFORM + ".zip")
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as package:
                package.write(binary, NAME)
                if traversal:
                    package.writestr("../outside", "invalid archive")
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            # Future worker compatibility must not prevent replacing an older CLI.
            asset = dict(version=version, url=base + f"/download/v{version}/" + archive.name,
                         sha256=digest, archive="zip", entrypoint=NAME,
                         cli_compat=f">={version}, <0.2.0", protocol_version=99)
            (release / "manifest.json").write_text(json.dumps(dict(schema_version=1, components={"cli:" + PLATFORM: asset})))
            (release / "SHA256SUMS").write_text(digest + "  " + archive.name + "\n")
            latest = web / "releases/latest/download"
            latest.mkdir(parents=True, exist_ok=True)
            (latest / "release-version.txt").write_text(version + "\n")
            return archive

        def installation(name):
            case = root / name
            target = case / "installed" / NAME
            target.parent.mkdir(parents=True)
            shutil.copy2(fixture_cli, target)
            env = dict(os.environ, SQLX_UPDATE_DIR=str(case / "update-state"), SQLX_UPDATE_RELEASE_BASE=base,
                       SQLX_DATA_DIR=str(case / "data"), SQLX_NO_UPDATE_CHECK="1", NO_PROXY="127.0.0.1,localhost")
            return case, target, env

        def call(target, env, *args, ok=True):
            started = time.monotonic()
            with subprocess.Popen([str(target), *args], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as process:
                while True:
                    try:
                        out, err = process.communicate(timeout=5)
                        break
                    except subprocess.TimeoutExpired:
                        elapsed = time.monotonic() - started
                        records = [json.loads(p.read_text()) for p in Path(env["SQLX_UPDATE_DIR"]).glob("*/install.json")]
                        diagnostic = dict(case=target.parent.parent.name, elapsed=round(elapsed, 1),
                                          executable_bytes=target.stat().st_size, install=records,
                                          requests=requests[-4:])
                        if platform.system() == "Linux":
                            stat = Path(f"/proc/{process.pid}/stat").read_text().rsplit(")", 1)[1].split()
                            diagnostic["cpu_seconds"] = round((int(stat[11]) + int(stat[12])) / os.sysconf("SC_CLK_TCK"), 1)
                        print(json.dumps(diagnostic), flush=True)
                        if elapsed >= 35:
                            process.kill()
                            out, err = process.communicate(timeout=5)
                            raise AssertionError(f"update command timed out: {out} {err}")
                result = subprocess.CompletedProcess(process.args, process.returncode, out, err)
            if not result.stdout.strip() and ok:
                assert result.returncode == 0, result.stderr
                return None
            value = json.loads(result.stdout)
            assert (result.returncode == 0) == ok, value
            return value.get("data", value)

        def unchanged(target, before):
            assert hashlib.sha256(target.read_bytes()).hexdigest() == before
            assert subprocess.check_output([str(target), "--version"], text=True).strip() == f"sqlx {CURRENT} (OtterMind/sqlx)"

        try:
            publish()
            case, target, env = installation("checks")
            before = hashlib.sha256(target.read_bytes()).hexdigest()
            value = call(target, env, "update", "check")
            assert value["status"] == "update_available" and value["latest_version"] == NEXT
            assert not (case / "data").exists(), "checking initialized datasource storage"
            count = len(requests)
            assert call(target, env, "update", "status")["last_check"]["status"] == "update_available"
            call(target, env, "update", "background-check")
            assert len(requests) == count, "status or fresh background cache caused network activity"
            cache = next((case / "update-state").glob("*/check.json"))
            cached = json.loads(cache.read_text())
            cached["checked_at"] = 0
            cache.write_text(json.dumps(cached))
            call(target, env, "update", "background-check")
            assert len(requests) > count

            ordinary, ordinary_cli, ordinary_env = installation("noninteractive")
            ordinary_env.pop("SQLX_NO_UPDATE_CHECK")
            count = len(requests)
            call(ordinary_cli, ordinary_env, "datasource", "list")
            assert len(requests) == count and not (ordinary / "update-state").exists()
            if os.name != "nt":
                import pty
                automatic, auto_cli, auto_env = installation("interactive")
                auto_env.pop("SQLX_NO_UPDATE_CHECK")
                master, slave = pty.openpty()
                try:
                    process = subprocess.Popen([str(auto_cli), "datasource", "list"], env=auto_env, stdin=slave, stdout=slave, stderr=slave)
                    assert process.wait(timeout=10) == 0
                    for _ in range(100):
                        observed = list((automatic / "update-state").glob("*/check.json"))
                        if observed and json.loads(observed[0].read_text())["status"] == "update_available":
                            break
                        time.sleep(.05)
                    else:
                        raise AssertionError("interactive background check did not finish")
                finally:
                    os.close(master)
                    os.close(slave)

            archive = publish(fixture_cli, CURRENT)
            count = len([path for path in requests if path.endswith(".zip")])
            assert call(target, env, "update", "install")["status"] == "up_to_date"
            assert len([path for path in requests if path.endswith(".zip")]) == count
            publish(fixture_cli, "0.1.1")
            assert call(target, env, "update", "install", "--version", "0.1.1", ok=False)["error"]["code"] == "update.downgrade_not_supported"
            unchanged(target, before)
            # A cached success must not hide a failed fresh check.
            latest = web / "releases/latest/download/release-version.txt"
            latest.unlink()
            call(target, env, "update", "check", ok=False)
            assert call(target, env, "update", "status")["last_check"]["status"] == "check_failed"

            for label, binary, expected in [
                ("wrong_product", wrong, "verification_failed"),
                ("post_install_failure", final_failure, "verification_failed"),
                ("hung_candidate", timeout, "verification_failed"),
                ("bad_checksum", good, "checksum_mismatch"),
                ("bad_archive", good, "invalid_archive"),
            ]:
                archive = publish(binary, traversal=label == "bad_archive")
                if label == "bad_checksum":
                    archive.write_bytes(archive.read_bytes() + b"corrupted in transit")
                case, target, env = installation(label)
                before = hashlib.sha256(target.read_bytes()).hexdigest()
                result = call(target, env, "update", "install", ok=False)
                assert result["error"]["code"] == "update." + expected, result
                unchanged(target, before)
                assert call(target, env, "update", "status")["last_install"]["status"] == "failed"
                assert not list(case.rglob("outside"))
                print(label + ": original executable preserved/restored", flush=True)

            publish()
            case, target, env = installation("success")
            # Invalid encrypted content must not be read or altered by the updater.
            data = case / "data"
            data.mkdir()
            sentinels = {"master.key": b"do not replace", "datasources.enc": b"do not parse", "identity.json": b"do not initialize"}
            for name, value in sentinels.items():
                (data / name).write_bytes(value)
            result = call(target, env, "update", "install")
            assert result["status"] == "installed" and result["to_version"] == NEXT
            assert subprocess.check_output([str(target), "--version"], text=True).strip() == f"sqlx {NEXT} (OtterMind/sqlx)"
            for name, value in sentinels.items():
                assert (data / name).read_bytes() == value
            receipt = json.loads(next((case / "update-state").glob("*/install.json")).read_text())
            assert receipt["status"] == "installed"

            # The lock only serializes updaters; an older command can finish normally.
            case, target, env = installation("concurrent")
            held = subprocess.Popen([str(target), "datasource", "add", "--name", "held", "--connection-stdin"], env=env,
                                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            delayed_download = True
            update = subprocess.Popen([str(target), "update", "install"], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            assert download_started.wait(10)
            assert call(target, env, "update", "install", ok=False)["error"]["code"] == "update.in_progress"
            out, err = update.communicate(timeout=35)
            assert update.returncode == 0 and json.loads(out)["data"]["status"] == "installed", (out, err)
            assert held.poll() is None, "the updater terminated another CLI command"
            out, err = held.communicate(json.dumps(dict(database_type="mysql", host="127.0.0.1", port=3306, username="fixture", password="test-only", tls="disable")), timeout=20)
            assert held.returncode == 0 and json.loads(out)["success"], (out, err)
            delayed_download = False
            print("Updates: fresh checks, cache/status, compatibility, no-op/downgrade, checksum/archive rejection, pre/post verification, timeout/rollback, real replacement, data preservation and concurrent invocations passed", flush=True)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()


if __name__ == "__main__":
    main()
