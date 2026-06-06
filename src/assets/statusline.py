#!/usr/bin/env python3
"""Claude Code status line.

Reads the session JSON on stdin and prints a single line:

    <model> · <dir> (<branch>) · ctx <pct>%

Absent or null fields are omitted. Must stay fast — the status line
re-runs after every assistant message (debounced at 300ms).
"""

from __future__ import annotations

import json
import os
import subprocess
import sys

try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)


def get(*path: str):
    """Walk nested dict keys, returning None if any segment is missing."""
    cur = data
    for key in path:
        if isinstance(cur, dict) and key in cur:
            cur = cur[key]
        else:
            return None
    return cur


model = get("model", "display_name") or get("model", "id") or "?"
cwd = get("workspace", "current_dir") or get("cwd") or os.getcwd()
dirname = os.path.basename(cwd.rstrip("/")) or cwd

branch = None
try:
    result = subprocess.run(
        ["git", "-C", cwd, "rev-parse", "--abbrev-ref", "HEAD"],
        capture_output=True,
        text=True,
        timeout=1,
    )
    if result.returncode == 0:
        branch = result.stdout.strip()
except Exception:
    pass

parts = [model, f"{dirname} ({branch})" if branch else dirname]

pct = get("context_window", "used_percentage")
if isinstance(pct, (int, float)):
    parts.append(f"ctx {pct:.0f}%")

print(" · ".join(parts))
