#!/usr/bin/env python3
"""Checks bwrap sandbox invariants for a Claude Code session.

Run inside the container by asking Claude to execute it:

    ~/sandbox-checks.py

To run without bwrap (override the container entrypoint on the host):

    container run --rm -it --entrypoint /bin/bash -v ...  <image>
    python3 /home/claude/sandbox-checks.py
"""

from __future__ import annotations

import os
import socket
import sys

# ── Formatting ──────────────────────────────────────────────────────────

passed = 0
failed = 0
sections: dict[str, bool] = {}
_current_section = ""


def check(ok: bool, description: str, detail: str = "") -> None:
    """Record and print one invariant check."""
    global passed, failed
    if ok:
        passed += 1
        tag = "PASS"
    else:
        failed += 1
        tag = "FAIL"
        if _current_section:
            sections[_current_section] = False
    suffix = f"  {detail}" if detail else ""
    print(f"  {tag}  {description}{suffix}")


def heading(title: str, key: str = "") -> None:
    global _current_section
    _current_section = key
    if key:
        sections.setdefault(key, True)
    print(f"\n{title}\n")


# ── 1. Sandbox summary ────────────────────────────────────────────────
#
# Parse /proc/self/mountinfo and environment to show everything bwrap
# has wrapped.  This section is informational — it never fails.

heading("Sandbox overview (from /proc/self/mountinfo + environment)")


def _parse_mounts() -> tuple[list[tuple[str, str, str]], list[tuple[str, str, str]]]:
    """Return (ro_mounts, rw_mounts) as lists of (mount_point, source, fstype)."""
    ro: list[tuple[str, str, str]] = []
    rw: list[tuple[str, str, str]] = []
    with open("/proc/self/mountinfo") as f:
        for line in f:
            parts = line.split()
            mount_point = parts[4]
            options = parts[5]
            source = parts[3]
            # fstype is after the " - " separator
            sep = parts.index("-")
            fstype = parts[sep + 1]
            if source == "/null":
                label = "/dev/null"
            elif fstype == "proc":
                label = "proc"
            elif fstype == "tmpfs":
                label = "tmpfs"
            elif fstype == "devtmpfs":
                label = "devtmpfs"
            elif fstype in ("virtiofs", "ext4"):
                label = "bind"
            else:
                label = fstype
            entry = (mount_point, label, fstype)
            if "ro" in options.split(","):
                ro.append(entry)
            else:
                rw.append(entry)
    return ro, rw


try:
    ro_mounts, rw_mounts = _parse_mounts()

    print("  Writable mounts:\n")
    for mount_point, label, _ in sorted(rw_mounts):
        print(f"    rw  {mount_point}  ({label})")

    print(f"\n  Read-only mounts:\n")
    for mount_point, label, _ in sorted(ro_mounts):
        print(f"    ro  {mount_point}  ({label})")

    print(f"\n  Network:\n")
    for var in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "GIT_SSH_COMMAND"):
        val = os.environ.get(var, "<unset>")
        print(f"    {var}={val}")

    print(f"\n  Process:\n")
    print(f"    SANDBOX_RUNTIME={os.environ.get('SANDBOX_RUNTIME', '<unset>')}")
    try:
        with open("/proc/self/status") as f:
            for line in f:
                if line.startswith("NSpid:"):
                    pids = line.split()[1:]
                    print(f"    PID namespace: {'isolated' if len(pids) > 1 else 'host'} (NSpid: {' '.join(pids)})")
                    break
    except OSError:
        pass

    print(f"\n  Totals: {len(rw_mounts)} rw, {len(ro_mounts)} ro mount(s)")

except OSError as e:
    print(f"  WARN  could not read /proc/self/mountinfo: {e}")


# ── 2. Network isolation ─────────────────────────────────────────────
#
# bwrap --unshare-net places the process in a network namespace with no
# interfaces.  DNS resolution and TCP connections to external hosts fail.

heading("Outbound network is blocked", "network")

socket.setdefaulttimeout(3)


def tcp_blocked(host: str, port: int) -> tuple[bool, str]:
    """Return (blocked, error_class) for a TCP connection attempt."""
    try:
        socket.create_connection((host, port))
        return False, "connected"
    except Exception as e:
        return True, type(e).__name__


def dns_blocked(host: str) -> tuple[bool, str]:
    """Return (blocked, error_class) for a DNS lookup."""
    try:
        addr = socket.getaddrinfo(host, 443)[0][4][0]
        return False, f"resolved to {addr}"
    except Exception as e:
        return True, type(e).__name__


for host in ["github.com", "pypi.org", "google.com"]:
    ok, detail = dns_blocked(host)
    check(ok, f"DNS unreachable: {host}", detail)

for host, port, label in [
    ("github.com", 443, "GitHub HTTPS"),
    ("github.com", 22, "GitHub SSH"),
    ("pypi.org", 443, "PyPI"),
    ("registry.npmjs.org", 443, "npm"),
    ("1.1.1.1", 53, "Cloudflare DNS"),
    ("8.8.8.8", 53, "Google DNS"),
]:
    ok, detail = tcp_blocked(host, port)
    check(ok, f"TCP unreachable: {label} ({host}:{port})", detail)


# ── 3. Filesystem isolation ──────────────────────────────────────────
#
# bwrap --ro-bind / / mounts the root filesystem read-only, then
# selectively re-mounts writable paths:
#
#   --bind /home/claude /home/claude
#   --bind /tmp/claude  /tmp/claude

def check_writable(path: str, expect_writable: bool) -> None:
    """Probe whether *path* is writable and compare against expectation."""
    probe = os.path.join(path, ".sandbox-probe")
    try:
        with open(probe, "w") as f:
            f.write("test")
        os.remove(probe)
        label = f"{path} is writable"
        check(expect_writable, label)
    except OSError as e:
        label = f"{path} is {'writable' if expect_writable else 'read-only'}"
        check(not expect_writable, label, type(e).__name__)


heading("System paths are read-only", "fs_ro")

for path in ["/etc", "/usr", "/usr/bin", "/var"]:
    check_writable(path, expect_writable=False)


heading("Work paths are writable", "fs_rw")

for path in ["/home/claude", "/home/claude/code", "/tmp/claude"]:
    check_writable(path, expect_writable=True)


# ── 4. Process isolation ─────────────────────────────────────────────

heading("Process runs in sandbox", "process")

check(
    os.environ.get("SANDBOX_RUNTIME") == "1",
    "SANDBOX_RUNTIME=1",
    f"value={os.environ.get('SANDBOX_RUNTIME', '<unset>')}",
)


# ── 5. Network proxy ─────────────────────────────────────────────────
#
# bwrap --setenv routes proxy variables to localhost socat relays that
# forward through a UNIX socket to Claude Code's proxy.  Allowed traffic
# (e.g. crates.io) passes through; everything else is blocked.
# GIT_SSH_COMMAND routes git-over-SSH through the same proxy.

heading("Allowed traffic is mediated by proxy", "proxy")

for var, expected_prefix in [
    ("HTTP_PROXY", "http://localhost:"),
    ("HTTPS_PROXY", "http://localhost:"),
    ("ALL_PROXY", "socks5h://localhost:"),
]:
    val = os.environ.get(var, "")
    check(val.startswith(expected_prefix), f"{var} points to local proxy", val or "<unset>")

check(
    "ProxyCommand" in os.environ.get("GIT_SSH_COMMAND", ""),
    "SSH is routed through proxy",
    os.environ.get("GIT_SSH_COMMAND", "<unset>"),
)


# ── Summary ────────────────────────────────────────────────────────────

def _ok(key: str) -> str:
    return "ok" if sections.get(key, False) else "FAIL"

print(f"\n{'─' * 40}")
print(f"  Network:     {_ok('network'):4s}  outbound DNS and TCP blocked")
_fs = "ok" if sections.get("fs_ro", False) and sections.get("fs_rw", False) else "FAIL"
print(f"  Filesystem:  {_fs:4s}  / read-only, workdir writable")
print(f"  Process:     {_ok('process'):4s}  SANDBOX_RUNTIME=1")
print(f"  Proxy:       {_ok('proxy'):4s}  HTTP, HTTPS, and SSH via localhost")
print(f"{'─' * 40}")
print(f"{passed} passed, {failed} failed")
sys.exit(1 if failed else 0)
