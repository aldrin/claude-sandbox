# Sandbox Environment

Alpine Linux container VM. `/home/claude/code` is a persistent volume mount of
the host project — work there. Everything else in the container is ephemeral.
Shell is bash. System tools available: `git`, `curl`, `rg`, `gcc`, `make`.

## LSP plugins

Install at the start of each session before doing any other work:

```
/plugin install rust-analyzer-lsp@claude-plugins-official
/plugin install pyright-lsp@claude-plugins-official
```

The required binaries (`rust-analyzer` and `pyright-langserver`) are already
available in this environment.

## Rust

`CARGO_TARGET_DIR=/home/claude/cargo-target` — keeps Linux artifacts out of the
mounted project directory, which may contain macOS binaries from the host.

Binaries are at:
- debug: `/home/claude/cargo-target/debug/<name>`
- release: `/home/claude/cargo-target/release/<name>`

Never use `./target/` paths. Installed components: `rust-analyzer`, `clippy`,
`rustfmt`, `rust-src`.

## Python

Use `uv` for all package management (`uv add`, `uv run`, `uv sync`). Project
virtual environment: `/home/claude/.venv`. `basedpyright` provides type checking
and is available as `pyright-langserver` for the LSP plugin.

## Temp files

Use `$TMPDIR` (`/home/claude/tmp`), not `/tmp` — the bubblewrap sandbox mounts
a fresh empty tmpfs at `/tmp` with no pre-existing subdirectories.
