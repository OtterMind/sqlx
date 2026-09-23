#!/usr/bin/env python3
"""Black-box checks for the Codex and Claude SQLX plugin launchers."""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def executable(path: Path, body: str) -> None:
    path.write_text(body, encoding="utf-8")
    path.chmod(0o755)


def fake_sqlx(path: Path) -> None:
    executable(
        path,
        """#!/bin/sh
if [ "$1" = "--version" ]; then
  printf 'sqlx %s (OtterMind/sqlx)\\n' "$FAKE_SQLX_VERSION"
  exit 0
fi
if [ "$1" = "mcp" ]; then
  printf 'mcp:%s\\n' "$FAKE_SQLX_VERSION"
  exit 0
fi
exit 64
""",
    )


def fake_npx(path: Path) -> None:
    executable(
        path,
        """#!/bin/sh
printf '%s %s\\n' "$SQLX_VERSION" "$*" > "$SQLX_NPX_LOG"
mkdir -p "$SQLX_INSTALL_DIR"
cat > "$SQLX_INSTALL_DIR/sqlx" <<EOF
#!/bin/sh
if [ "\$1" = "--version" ]; then
  printf 'sqlx %s (OtterMind/sqlx)\\n' "$SQLX_VERSION"
  exit 0
fi
if [ "\$1" = "mcp" ]; then
  printf 'mcp:%s\\n' "$SQLX_VERSION"
  exit 0
fi
exit 64
EOF
chmod 755 "$SQLX_INSTALL_DIR/sqlx"
""",
    )


def launcher(agent: str) -> Path:
    return ROOT / "integrations" / agent / "plugins" / "sqlx" / "bin" / "sqlx-mcp"


def runtime_metadata(agent: str) -> dict[str, object]:
    plugin = ROOT / "integrations" / agent / "plugins" / "sqlx"
    manifest_name = ".codex-plugin/plugin.json" if agent == "codex" else ".claude-plugin/plugin.json"
    plugin_manifest = json.loads((plugin / manifest_name).read_text(encoding="utf-8"))
    runtime = json.loads((plugin / "runtime.json").read_text(encoding="utf-8"))
    assert runtime["plugin_version"] == plugin_manifest["version"], (agent, runtime, plugin_manifest)
    assert runtime["cli_min_version"] == plugin_manifest["version"], (agent, runtime, plugin_manifest)
    return runtime


def run_launcher(path: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(path)],
        input="",
        text=True,
        capture_output=True,
        env=env,
        timeout=30,
        check=False,
    )


def run(agent: str, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return run_launcher(launcher(agent), env)


def main() -> None:
    if os.name == "nt":
        print("Plugin launcher black-box tests skipped on Windows")
        return

    with tempfile.TemporaryDirectory(prefix="sqlx-integrations-") as temporary:
        root = Path(temporary)
        fake_bin = root / "fake-bin"
        fake_bin.mkdir()
        fake_sqlx(fake_bin / "sqlx")
        fake_npx(fake_bin / "npx")

        for agent in ("codex", "claude"):
            runtime = runtime_metadata(agent)
            expected = runtime["cli_min_version"]
            common = os.environ.copy()
            common.update(
                {
                    "HOME": str(root / f"{agent}-home"),
                    "SQLX_INSTALL_DIR": str(root / f"{agent}-install"),
                    "PATH": f"{fake_bin}:/usr/bin:/bin",
                    # Far ahead of any release, so a version bump cannot collide with this fake.
                    "FAKE_SQLX_VERSION": "0.1.99",
                }
            )
            (root / f"{agent}-home").mkdir()
            result = run(agent, common)
            assert result.returncode == 0, (agent, result.stderr)
            assert result.stdout.strip() == "mcp:0.1.99", (agent, result.stdout)

            old_dir = root / f"{agent}-old-bin"
            install = root / f"{agent}-upgrade"
            log = root / f"{agent}-npx.log"
            old_dir.mkdir()
            fake_sqlx(old_dir / "sqlx")
            upgrade = common | {
                "FAKE_SQLX_VERSION": "0.1.9",
                "SQLX_INSTALL_DIR": str(install),
                "SQLX_NPX_LOG": str(log),
                "PATH": f"{old_dir}:{fake_bin}:/usr/bin:/bin",
            }
            result = run(agent, upgrade)
            assert result.returncode == 0, (agent, result.stderr)
            assert result.stdout.strip() == f"mcp:{expected}", (agent, result.stdout)
            assert log.read_text(encoding="utf-8").startswith(f"{expected} "), log.read_text()
            assert f"--target {agent}" in log.read_text(encoding="utf-8")

            broken_plugin = root / f"{agent}-broken-plugin"
            shutil.copytree(ROOT / "integrations" / agent / "plugins" / "sqlx", broken_plugin)
            broken_runtime_path = broken_plugin / "runtime.json"
            broken_runtime = json.loads(broken_runtime_path.read_text(encoding="utf-8"))
            broken_runtime["plugin_version"] = "0.1.13"
            broken_runtime_path.write_text(json.dumps(broken_runtime, indent=2) + "\n", encoding="utf-8")
            result = run_launcher(broken_plugin / "bin" / "sqlx-mcp", common)
            assert result.returncode == 78, (agent, result.returncode, result.stderr)
            assert "does not match plugin version" in result.stderr, (agent, result.stderr)

            explicit = upgrade | {"SQLX_BIN": str(old_dir / "sqlx")}
            result = run(agent, explicit)
            assert result.returncode == 78, (agent, result.returncode, result.stderr)
            assert "requires SQLX >=" in result.stderr, (agent, result.stderr)

    print("Plugin launchers: compatible CLI reuse, stale CLI bootstrap, and explicit override checks passed")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as error:
        print(error, file=sys.stderr)
        raise
