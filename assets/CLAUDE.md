# Sandbox Environment

**At the start of every session: read this entire file before doing any work.**
This is an ephemeral container — nothing outside `/home/claude/code` persists
between sessions. All operational knowledge lives here.

Ubuntu container VM. `/home/claude/code` is a persistent volume mount of
the host project — work there. Everything else in the container is ephemeral.
Shell is bash. System tools available: `git`, `rg`, `gcc`, `make`.

## Intent

Your role is to make code changes. The host user owns the git history and all
network operations. Focus on reading, editing, building, and testing code within
the mounted project directory.

## Committing changes

The host user owns the git history. When work is ready to commit:
1. Stage changes with `git add`
2. Propose a commit message for the user to run on the host

Commit messages must follow these rules:
1. Separate subject from body with a blank line
2. Limit the subject line to 50 characters
3. Capitalize the subject line
4. Do not end the subject line with a period
5. Use the imperative mood ("Fix bug" not "Fixed bug")
6. Wrap the body at 72 characters
7. Use the body to explain what and why, not how

Do not use conventional commit prefixes (feat:, fix:, chore:, etc.).
Avoid listing implementation details; the code speaks for itself.

## Dependencies

Always ask before adding a new dependency to any project. The user reviews all
dependencies before they are added. This applies to all package ecosystems:
Cargo, npm, pip, and any other package manager.

## Rust

Binaries are at:
- debug: `/home/claude/cargo-target/debug/<name>`
- release: `/home/claude/cargo-target/release/<name>`

Never use `./target/` paths — `CARGO_TARGET_DIR` is set automatically.

## Python

Use `uv` for all package management (`uv add`, `uv run`, `uv sync`).

## DuckDB

Prefer `duckdb` over loading data into Python with pandas or similar for
querying CSV, Parquet, JSON, or other data files.
