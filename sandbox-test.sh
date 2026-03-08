#!/usr/bin/env bash
# sandbox-test.sh — verify Claude Code sandbox enforcement inside the container.
#
# Claude Code enforces three permission modalities:
#
#   allowed          — runs automatically, no prompt (allow list or autoAllowBashIfSandboxed)
#   request_permission — Claude Code prompts the user to approve before running
#   denied           — automatically rejected without prompting (deny list)
#
# Current settings (see ~/.claude/settings.json):
#   defaultMode: acceptEdits        → file edits are auto-accepted
#   autoAllowBashIfSandboxed: true  → all Bash runs without prompting when sandboxed
#   allow list: WebSearch, WebFetch(crates.io), two AWS MCP doc tools
#   deny list:  empty
#
# This script tests what it can verify automatically (filesystem permissions,
# network reachability from the container) and documents the three modalities
# for operations that require Claude Code context to verify.
#
# Run this directly in the container shell: bash sandbox-test.sh

PASS=0
FAIL=0

green='\033[0;32m'
red='\033[0;31m'
yellow='\033[0;33m'
bold='\033[1m'
dim='\033[2m'
reset='\033[0m'

section() { echo; echo -e "${bold}$1${reset}"; }
note()    { echo -e "  ${dim}note: $1${reset}"; }

pass()  { echo -e "  ${green}PASS${reset}  $1"; PASS=$((PASS + 1)); }
fail()  { echo -e "  ${red}FAIL${reset}  $1"; FAIL=$((FAIL + 1)); }
info()  { echo -e "  ${yellow}INFO${reset}  $1"; }

expect_allowed() {
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then pass "allowed: $desc"
    else                         fail "should be allowed but was blocked: $desc"
    fi
}

expect_blocked() {
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then fail "should be blocked but was allowed: $desc"
    else                          pass "blocked: $desc"
    fi
}

tcp_reachable() {
    local host="$1" port="$2"
    python3 - <<EOF
import socket, sys
try:
    socket.setdefaulttimeout(3)
    socket.create_connection(('$host', $port))
    sys.exit(0)
except Exception:
    sys.exit(1)
EOF
}


# =============================================================================
section "Container network — all outbound blocked"
# =============================================================================
# The container has no outbound network access from a direct shell.
# The crates.io whitelist only applies to commands run via Claude Code's
# Bash tool, not to processes launched directly in the container.

expect_blocked "pypi.org:443"            tcp_reachable pypi.org 443
expect_blocked "registry.npmjs.org:443"  tcp_reachable registry.npmjs.org 443
expect_blocked "github.com:443"          tcp_reachable github.com 443
expect_blocked "github.com:22 (SSH)"     tcp_reachable github.com 22
expect_blocked "index.crates.io:443"     tcp_reachable index.crates.io 443
expect_blocked "static.crates.io:443"    tcp_reachable static.crates.io 443

note "crates.io IS reachable when Claude runs cargo/curl via the Bash tool (tool-layer whitelist)"


# =============================================================================
section "Filesystem writes — blocked"
# =============================================================================

expect_blocked "write /etc/"                     test -w /etc/
expect_blocked "write /usr/"                     test -w /usr/
expect_blocked "write ~/.claude/settings.json"   test -w "${HOME}/.claude/settings.json"


# =============================================================================
section "Filesystem writes — allowed"
# =============================================================================

expect_allowed "write ~/tmp/"             test -w "${HOME}/tmp/"
expect_allowed "write ~/"                test -w "${HOME}/"
expect_allowed "write ~/code/ (project)" test -w "${HOME}/code/"
expect_allowed "write ~/.claude/ (other files)" test -w "${HOME}/.claude/"


# =============================================================================
section "Filesystem reads — unrestricted"
# =============================================================================

expect_allowed "read /etc/passwd"              test -r /etc/passwd
expect_allowed "read /etc/shadow (if exists)"  test -e /etc/shadow && test -r /etc/shadow || pass "read /etc/shadow (no shadow file)"
expect_allowed "read ~/.claude/settings.json"  test -r "${HOME}/.claude/settings.json"


# =============================================================================
section "Git — local operations"
# =============================================================================
# ~/.gitconfig is inaccessible in this environment; use GIT_CONFIG_NOSYSTEM
# and a writable HOME override to allow git to run.

git_cmd() { GIT_CONFIG_NOSYSTEM=1 HOME=/tmp git -C "${HOME}/code" "$@"; }

expect_allowed "git status"    bash -c "GIT_CONFIG_NOSYSTEM=1 HOME=/tmp git -C ${HOME}/code status"
expect_allowed "git log"       bash -c "GIT_CONFIG_NOSYSTEM=1 HOME=/tmp git -C ${HOME}/code log --oneline -1"
expect_allowed "git diff"      bash -c "GIT_CONFIG_NOSYSTEM=1 HOME=/tmp git -C ${HOME}/code diff"
expect_allowed "git remote -v" bash -c "GIT_CONFIG_NOSYSTEM=1 HOME=/tmp git -C ${HOME}/code remote -v"
expect_blocked "git fetch (remote unreachable)" \
    bash -c "GIT_CONFIG_NOSYSTEM=1 HOME=/tmp git -C ${HOME}/code fetch --dry-run"


# =============================================================================
section "Claude Code permission modalities (informational)"
# =============================================================================
# These cannot be automatically verified from a shell — they require observing
# Claude Code's behavior. Confirmed by inspection of settings.json.

info "ALLOWED (auto, no prompt):  Bash commands          [autoAllowBashIfSandboxed=true]"
info "ALLOWED (auto, no prompt):  file Read/Edit/Write   [defaultMode=acceptEdits]"
info "ALLOWED (auto, no prompt):  WebSearch              [explicit allow list]"
info "ALLOWED (auto, no prompt):  WebFetch *.crates.io   [explicit allow list]"
info "REQUEST_PERMISSION:         WebFetch other domains  [not in allow or deny]"
info "REQUEST_PERMISSION:         MCP tools (non-AWS doc) [not in allow or deny]"
info "DENIED (auto, no prompt):   nothing                [deny list is empty]"
info "NOTE: ~/.claude/settings.local.json is writable at OS level but protected"
info "      by Claude Code's tool layer (denyWithinAllow) — not testable from shell"
echo
note "deny list was removed — it had no effect while autoAllowBashIfSandboxed=true"


# =============================================================================
echo
echo -e "${bold}Results: ${green}${PASS} passed${reset}${bold}, ${red}${FAIL} failed${reset}"
if [ "$FAIL" -eq 0 ]; then
    echo -e "${green}All verifiable sandbox boundaries confirmed.${reset}"
else
    echo -e "${red}Some boundaries are not enforced as expected.${reset}"
fi
