# claude-sandbox

Single-binary Rust CLI that launches Claude Code inside isolated Apple
container VMs. All logic is in `src/main.rs`; templates are embedded
from `src/assets/` via `include_str!()`.

## Commands

`init` scaffolds `.claude-sandbox/`, `build` creates the container
image, `run` launches Claude Code inside it, `status` checks for asset
drift after a binary update.

## Build and test

```
make check    # fmt + lint + test + cov (gate before commits)
make install  # cargo install --path .
```

## Plugin sync rule

`enabledPlugins` in `settings.json` and the `claude plugin install`
calls in `Containerfile` must list the same set of plugins. Adding or
removing a plugin requires updating both files.
