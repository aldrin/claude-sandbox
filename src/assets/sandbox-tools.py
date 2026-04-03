#!/usr/bin/env python3
"""Checks that the container image has the expected toolchain and plugins.

Run inside the container by asking Claude to execute it:

    ~/sandbox-tools.py
"""

from __future__ import annotations

import json
import os
import subprocess
import sys

# ── Formatting ──────────────────────────────────────────────────────────

passed = 0
failed = 0


def check(ok: bool, description: str, detail: str = "") -> None:
    """Record and print one check."""
    global passed, failed
    if ok:
        passed += 1
        tag = "PASS"
    else:
        failed += 1
        tag = "FAIL"
    suffix = f"  {detail}" if detail else ""
    print(f"  {tag}  {description}{suffix}")


def heading(title: str) -> None:
    print(f"\n{title}\n")


# ── 1. Toolchain ──────────────────────────────────────────────────────
#
# Tools installed by the Containerfile.  Present inside and outside bwrap.

heading("Development toolchain is available")

tools = [
    (["rustc", "--version"], "Rust compiler"),
    (["cargo", "--version"], "Cargo"),
    (["rust-analyzer", "--version"], "rust-analyzer"),
    (["cargo", "clippy", "--version"], "Clippy"),
    (["rustfmt", "--version"], "rustfmt"),
    (["python3", "--version"], "Python"),
    (["uv", "--version"], "uv"),
    (["basedpyright", "--version"], "basedpyright"),
    (["duckdb", "--version"], "DuckDB"),
    (["pandoc", "--version"], "Pandoc"),
    (["typst", "--version"], "Typst"),
    (["emacs", "--version"], "Emacs"),
    (["rg", "--version"], "ripgrep"),
    (["bwrap", "--version"], "bubblewrap"),
]

for cmd, label in tools:
    result = subprocess.run(cmd, capture_output=True, text=True)
    version = (result.stdout.strip() or result.stderr.strip()).split("\n")[0]
    check(result.returncode == 0, label, version)


# ── 2. Plugins ────────────────────────────────────────────────────────
#
# Plugins installed via `claude plugin install` in the Containerfile and
# declared active in settings.json via `enabledPlugins`.  Both lists must
# stay in sync: installed but not enabled triggers recommendation prompts;
# enabled but not installed causes errors at startup.

heading("Plugins are installed and active")

_plugins_file = os.path.expanduser("~/.claude/plugins/installed_plugins.json")
try:
    _installed = json.loads(open(_plugins_file).read()).get("plugins", {})
except Exception:
    _installed = {}

for plugin in ["rust-analyzer-lsp", "pyright-lsp", "code-simplifier", "feature-dev"]:
    key = f"{plugin}@claude-plugins-official"
    check(key in _installed, f"{plugin} is installed")


# ── 3. Environment ────────────────────────────────────────────────────
#
# Variables set in the Containerfile and settings.json.  Present inside
# and outside bwrap, except TMPDIR which bwrap overrides to /tmp/claude.

heading("Runtime environment is configured")

for var, expected in [
    ("CARGO_TARGET_DIR", "/home/claude/cargo-target"),
    ("TMPDIR", "/tmp/claude"),
    ("EDITOR", "emacs"),
    ("TERM", "xterm-256color"),
]:
    val = os.environ.get(var)
    check(val == expected, f"{var}={expected}", f"actual={val}")


# ── Summary ────────────────────────────────────────────────────────────

print(f"\n{'─' * 40}")
print(f"{passed} passed, {failed} failed")
sys.exit(1 if failed else 0)
