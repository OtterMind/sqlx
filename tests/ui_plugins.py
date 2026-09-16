#!/usr/bin/env python3
"""Public CLI/plugin contract: installation, switching, compatibility and integrity."""
import functools
import hashlib
import http.cookiejar
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def main():
    with tempfile.TemporaryDirectory(prefix="sqlx-ui-plugins-") as directory:
        temp = Path(directory)
        data = temp / "data"
        cli = ROOT / "target/debug" / ("sqlx.exe" if os.name == "nt" else "sqlx")
        env = dict(os.environ, SQLX_DATA_DIR=str(data), SQLX_WORKER_DIR=str(ROOT / "target/debug"))

        def command(*args, ok=True):
            result = subprocess.run([str(cli), "--no-open", *args], env=env, capture_output=True, text=True, timeout=40)
            value = json.loads(result.stdout)
            assert (result.returncode == 0) == ok, value
            return value.get("data", value)

        source = temp / "source"
        shutil.copytree(ROOT / "examples/terminal-ui/dist", source)
        original = json.loads((source / "ui-plugin.json").read_text())
        for change in [{"api_version": 99}, {"cli_compat": ">=99.0.0"}, {"id": "../escape"}, {"capabilities": []}]:
            (source / "ui-plugin.json").write_text(json.dumps(dict(original, **change)))
            command("ui", "plugin", "install", "--path", str(source), ok=False)
        (source / "ui-plugin.json").write_text(json.dumps(original))
        (source / "unsafe.exe").write_bytes(b"not a web asset")
        command("ui", "plugin", "install", "--path", str(source), ok=False)
        (source / "unsafe.exe").unlink()
        web = temp / "web"
        web.mkdir()
        archive = web / "terminal.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as package:
            for file in source.iterdir():
                package.write(file, file.name)
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Quiet, directory=str(web)))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        url = f"http://127.0.0.1:{server.server_port}/terminal.zip"
        try:
            command("ui", "plugin", "install", "--url", url, "--sha256", "0" * 64, ok=False)
            assert command("ui", "plugin", "list")["plugins"] == []
            command("ui", "plugin", "install", "--url", url, "--sha256", digest)
            assert not command("ui", "plugin", "list")["plugins"][0]["active"]
            command("ui", "plugin", "install", "--path", str(source))  # Same content is idempotent.
            (source / "app.css").write_text("body {}")
            command("ui", "plugin", "install", "--path", str(source), ok=False)
            default_version=json.loads((ROOT / "ui/dist/ui-plugin.json").read_text())["version"]
            default_base=f"/_ui/default/{default_version}"
            command("ui", "plugin", "install", "--path", str(ROOT / "ui/dist"))
            command("ui", "plugin", "use", "default")
            launch = command("ui")["url"]
            parsed = urllib.parse.urlsplit(launch)
            origin = parsed.scheme + "://" + parsed.netloc
            assert not parsed.fragment and not parsed.query, launch
            client = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

            def request(path, body=None, expected=200):
                headers = {"Origin": origin, "X-SQLX-UI": "1", "Content-Type": "application/json"}
                req = urllib.request.Request(origin + path, headers=headers, data=None if body is None else json.dumps(body).encode())
                try:
                    with client.open(req, timeout=10) as response:
                        code, raw = response.status, response.read()
                except urllib.error.HTTPError as failure:
                    code, raw = failure.code, failure.read()
                assert code == expected, (path, code, raw[:100])
                return raw

            assert (default_base+"/").encode() in request("/")
            old_script = request(default_base+"/app.js")
            command("ui", "plugin", "use", "terminal")
            assert json.loads(request("/api/plugin"))["id"] == "terminal"
            assert b"/_ui/terminal/0.1.1/" in request("/")
            assert request(default_base+"/app.js") == old_script
            request("/api/plugins/activate", {"id": "default", "version": "0.1.1"}, expected=404)
            request("/_ui/terminal/0.1.1/.sqlx-ui-receipt.json", expected=404)
            request("/_ui/terminal/0.1.1/%2e%2e/%2e%2e/active.json", expected=404)
            asset = data / "plugins/ui/terminal/0.1.1/app.js"
            previous = asset.read_bytes()
            asset.write_bytes(b"tampered")
            request("/_ui/terminal/0.1.1/app.js", expected=404)
            asset.write_bytes(previous)
            command("ui", "plugin", "remove", "default", "--version", default_version, ok=False)
            command("ui", "stop")
            for _ in range(200):
                if not (data / "ui/state.json").exists():
                    break
                time.sleep(.05)
            command("ui", "plugin", "remove", "terminal", "--version", "0.1.1", ok=False)
            command("ui", "plugin", "use", "default")
            command("ui", "plugin", "remove", "terminal", "--version", "0.1.1")
            assert len(command("ui", "plugin", "list")["plugins"]) == 1
            print("UI plugins: local/ZIP installation, checksums, compatibility, immutable versions, live switching, old assets, tampering and removal guards passed")
        finally:
            command("ui", "stop")
            server.shutdown()
            server.server_close()
            thread.join()


if __name__ == "__main__":
    main()
