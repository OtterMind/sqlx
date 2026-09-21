#!/usr/bin/env python3
"""Build the plugin-distribution tree that is published on the `plugins` branch.

The distribution branch is an orphan branch: it carries only the marketplace manifests and the
plugin shells, so Codex and Claude Code users install SQLX without cloning the source tree. This
script is the single source of truth for that branch; the `Plugins branch` workflow runs it and
pushes the result, so the branch can never drift from `integrations/`.
"""
import argparse
import json
import os
import re
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PLUGIN_VERSION_KEYS = ("version",)


def workspace_version(root: Path) -> str:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version = "([^"]+)"', text, re.MULTILINE)
    if match is None:
        raise SystemExit("cannot read [workspace.package] version from Cargo.toml")
    return match.group(1)


def copy_plugin(source: Path, target: Path, version: str) -> None:
    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(source, target)
    for path in target.rglob("plugin.json"):
        manifest = json.loads(path.read_text(encoding="utf-8"))
        for key in PLUGIN_VERSION_KEYS:
            if key in manifest:
                manifest[key] = version
        path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    launcher = target / "bin" / "sqlx-mcp"
    if launcher.is_file():
        os.chmod(launcher, 0o755)


def write_manifests(target: Path) -> None:
    codex = target / ".agents" / "plugins"
    codex.mkdir(parents=True, exist_ok=True)
    (codex / "marketplace.json").write_text(
        json.dumps(
            {
                "name": "ottermind",
                "interface": {"displayName": "OtterMind"},
                "plugins": [
                    {
                        "name": "sqlx",
                        "source": {"source": "local", "path": "./plugins/sqlx"},
                        "policy": {"installation": "AVAILABLE", "authentication": "ON_USE"},
                        "category": "Developer Tools",
                    }
                ],
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    claude = target / ".claude-plugin"
    claude.mkdir(parents=True, exist_ok=True)
    (claude / "marketplace.json").write_text(
        json.dumps(
            {
                "name": "ottermind",
                "owner": {"name": "OtterMind"},
                "description": (
                    "OtterMind plugins for Claude Code. Currently ships SQLX for MySQL, "
                    "PostgreSQL, Oracle and SQL Server."
                ),
                "plugins": [
                    {
                        "name": "sqlx",
                        "source": "./plugins/sqlx-claude",
                        "description": (
                            "List saved OtterMind SQLX datasources, test connections, "
                            "execute SQL and open local result pages."
                        ),
                    }
                ],
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


README = """# plugins branch

Distribution-only branch, generated from `integrations/` on `main` by
`.github/workflows/plugins-branch.yml`. Do not edit it by hand: the next sync overwrites it.

It carries the agent-harness marketplace manifests and the plugin shells so Codex and Claude Code
users install SQLX **without cloning the source tree**:

```sh
# Codex
codex plugin marketplace add OtterMind/sqlx@plugins
codex plugin add sqlx@ottermind

# Claude Code
claude plugin marketplace add OtterMind/sqlx@plugins     # or the /plugin equivalent
claude plugin install sqlx@ottermind
```

| Path | Purpose |
|---|---|
| `.agents/plugins/marketplace.json` | Codex marketplace (its native manifest) |
| `.claude-plugin/marketplace.json` | Claude Code marketplace |
| `plugins/sqlx/` | Codex plugin (`.codex-plugin/plugin.json`, `.mcp.json`, `bin/sqlx-mcp`) |
| `plugins/sqlx-claude/` | Claude Code plugin (`.claude-plugin/plugin.json`, `.mcp.json`, `bin/sqlx-mcp`) |

Both plugins launch `sqlx mcp`; the launcher uses `SQLX_BIN` when set, otherwise the first `sqlx`
on `PATH`. The CLI version in each plugin manifest matches the repository release that generated
the branch.
"""


def build(root: Path, target: Path) -> str:
    version = workspace_version(root)
    if target.exists():
        shutil.rmtree(target)
    target.mkdir(parents=True)
    copy_plugin(root / "integrations" / "codex" / "plugins" / "sqlx", target / "plugins" / "sqlx", version)
    copy_plugin(
        root / "integrations" / "claude" / "plugins" / "sqlx",
        target / "plugins" / "sqlx-claude",
        version,
    )
    write_manifests(target)
    (target / "README.md").write_text(README, encoding="utf-8")
    return version


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path, default=ROOT / "dist-plugins")
    args = parser.parse_args()
    version = build(args.root, args.output)
    files = sorted(p.relative_to(args.output).as_posix() for p in args.output.rglob("*") if p.is_file())
    print(f"Built {args.output} for version {version}:")
    for name in files:
        print(f"  {name}")


if __name__ == "__main__":
    main()
