# claude-sandbox

Claude Code is most useful when it can edit files and run commands on its own. You don't want to
give it access to everything on your machine, but you also don't want to babysit every action it
takes. `claude-sandbox` is how I balance this for my personal projects. It runs Claude Code in a
lightweight Linux VM (via [Apple Containers][containers] on macOS Tahoe) with only your project
directory mounted. Claude only sees the directory you started it in and nothing else on the host.

Inside the VM, Claude Code's [sandbox mode][sandbox] and [acceptEdits][permissions] let bash commands
run freely without prompting. As long as work stays local, you never see a permission dialog. If a
command tries to reach a domain that isn't allowlisted, the bubblewrap sandbox triggers a permission
prompt automatically. Most work doesn't need remote content, so most sessions run uninterrupted.

In short, `claude-sandbox` is a standalone CLI that runs Claude Code in an isolated Linux VM with
sandbox mode and acceptEdits, so bash runs freely inside [bubblewrap][bubblewrap] but anything that
reaches outside prompts you.

To use it, install with:

```bash
cargo install --git https://github.com/aldrin/claude-sandbox.git
```

### Usage

```
$ claude-sandbox --help
Launch Claude Code in a sandboxed Apple container VM.

Usage: claude-sandbox <COMMAND>

Commands:
  init    Initialize workspace with default Containerfile
  build   Build the sandbox container image
  run     Run Claude Code in the container
  status  Compare .claude-sandbox/ files against the current binary's embedded assets
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

The CLI has three primary commands. Go into a project directory and run `init` to scaffold a
`.claude-sandbox/` directory with the image definition and Claude Code configuration.
Run `build` to build the container image. After that, `run` is all you need to start
Claude Code inside the container.

Each project gets its own image, named `claude-sandbox-<dirname>` by default. You can
override this with `--name` during init. The name is saved in `.claude-sandbox/image-name`
and reused by `build` and `run`, so multiple sandboxes for different projects can run
at once.

- **`init`** scaffolds `.claude-sandbox/` in your project with a `Containerfile`
  (Ubuntu 26.04 with Claude Code and developer tooling), `settings.json` for permissions
  and sandbox config, and a `CLAUDE.md` to orient Claude inside the container. Edit the
  `Containerfile` to add what your project needs.

  <p align="center"><img src="init.svg"/></p>

- **`build`** builds the container image. Make sure Apple Containers is running first
  (`container system start`). The first build pulls in Claude Code and your tooling, so
  it takes a few minutes. Rebuild only when you change the `Containerfile`.

- **`run`** launches Claude Code in the container with your project mounted. It reads
  your OAuth token from the macOS keychain and passes it as an environment variable. If
  you haven't authenticated yet, run `claude auth login` first.

  <p align="center"><img src="run.svg"/></p>

  The container gets 2 CPUs and 4 GB of memory by default. `--cpus` and `--memory`
  accept values between 2 and 8. The container is destroyed on exit.

  > **Note:** The token never appears on the command line, but is visible via
  > `container inspect` and inside the container's environment.

  A `sandbox-checks.py` script in the container reports the isolation state.

  ```
  ❯ python3 ~/sandbox-checks.py

  Outbound network is blocked

    PASS  DNS unreachable: github.com
    PASS  DNS unreachable: pypi.org
    PASS  DNS unreachable: google.com
    PASS  TCP unreachable: GitHub HTTPS (github.com:443)
    PASS  TCP unreachable: GitHub SSH (github.com:22)
    PASS  TCP unreachable: PyPI (pypi.org:443)
    PASS  TCP unreachable: npm (registry.npmjs.org:443)
    PASS  TCP unreachable: Cloudflare DNS (1.1.1.1:53)
    PASS  TCP unreachable: Google DNS (8.8.8.8:53)

  System paths are read-only

    PASS  /etc is read-only
    PASS  /usr is read-only
    PASS  /usr/bin is read-only
    PASS  /var is read-only

  Work paths are writable

    PASS  /home/claude is writable
    PASS  /home/claude/code is writable
    PASS  /tmp/claude is writable

  Process runs in sandbox

    PASS  SANDBOX_RUNTIME=1

  Allowed traffic is mediated by proxy

    PASS  HTTP_PROXY points to local proxy
    PASS  HTTPS_PROXY points to local proxy
    PASS  ALL_PROXY points to local proxy
    PASS  SSH is routed through proxy

  ────────────────────────────────────────
    Network:     ok    outbound DNS and TCP blocked
    Filesystem:  ok    / read-only, workdir writable
    Process:     ok    SANDBOX_RUNTIME=1
    Proxy:       ok    HTTP, HTTPS, and SSH via localhost
  ────────────────────────────────────────
  21 passed, 0 failed
  ```

### What's in the container

The container is a workspace for writing and testing code with Claude. The default
`Containerfile` reflects my own toolchain. Edit it to fit yours. A bundled
`sandbox-tools.py` script shows what's installed.

  ```
  ❯ python3 ~/sandbox-tools.py

  Development toolchain is available

    PASS  Rust compiler  rustc 1.94.1
    PASS  Cargo  cargo 1.94.1
    PASS  rust-analyzer  rust-analyzer 1.94.1
    PASS  Clippy  clippy 0.1.94
    PASS  rustfmt  rustfmt 1.8.0-stable
    PASS  Python  Python 3.14.3
    PASS  uv  uv 0.11.3
    PASS  basedpyright  basedpyright 1.39.0
    PASS  DuckDB  v1.5.1
    PASS  Pandoc  pandoc 3.9.0.2
    PASS  Typst  typst 0.14.2
    PASS  Emacs  GNU Emacs 30.2
    PASS  ripgrep  ripgrep 15.1.0
    PASS  bubblewrap  bubblewrap 0.11.1

  Plugins are installed and active

    PASS  rust-analyzer-lsp is installed
    PASS  pyright-lsp is installed
    PASS  code-simplifier is installed
    PASS  feature-dev is installed

  Runtime environment is configured

    PASS  CARGO_TARGET_DIR=/home/claude/cargo-target
    PASS  TMPDIR=/tmp/claude
    PASS  EDITOR=emacs
    PASS  TERM=xterm-256color

  ────────────────────────────────────────
  22 passed, 0 failed
  ```

  Version numbers reflect the container image at build time and may differ from yours.

### Settings

`settings.json` controls permissions, sandbox mode, and the runtime environment. The
`env` block sets paths, terminal settings, and disables features that don't apply in an
ephemeral container (cron, background tasks, auto-memory). It also pre-approves plugins
for Rust, Python, and common workflows.

[containers]: https://github.com/apple/container
[sandbox]: https://code.claude.com/docs/en/sandboxing
[permissions]: https://code.claude.com/docs/en/permissions
[bubblewrap]: https://github.com/containers/bubblewrap
