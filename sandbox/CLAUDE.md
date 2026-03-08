# Sandbox Environment

Ubuntu container VM. `/home/claude/code` is a persistent volume mount of
the host project — work there. Everything else in the container is ephemeral.
Shell is bash. System tools available: `git`, `rg`, `gcc`, `make`.

## Intent

Your role is to make code changes. The host user owns the git history and all
network operations. Focus on reading, editing, building, and testing code within
the mounted project directory.

## Sandbox restrictions

The network sandbox only allows outbound connections to `index.crates.io` and
`static.crates.io`. Everything else is blocked at the network layer:

- Arbitrary network tools (`curl`, `wget` to non-whitelisted hosts, `ssh`, `nc`).
- Package registries: `pip install`, `npm install`, `npx`.
- Remote git operations: `git push`, `git pull`, `git fetch`, `git clone`.

Local git operations (`git status`, `git diff`, `git log`, `git add`, `git stash`,
`git remote`) are not restricted by the sandbox. `git commit` is disabled.

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
