//! claude-sandbox: Launch Claude Code in a sandboxed Apple container VM
//!
//! External commands used:
//! - container --version
//! - container build -t <image> -f <containerfile> <context>
//! - container run --rm -it -e <env> -m <memory> -c <cpus> -v <volume> <image>
//! - security find-generic-password -s <service> -w

use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output};

use anyhow::{bail, Context, Result};
use clap::Parser;
use log::{debug, info};

const SANDBOX_DIR: &str = ".claude-sandbox";
const IMAGE_NAME_FILE: &str = "image-name";
const IMAGE_PREFIX: &str = "claude-sandbox";
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const HINT_RUN_INIT: &str = "Run 'claude-sandbox init' first to initialize the workspace.";

#[derive(Parser)]
#[command(
    name = "claude-sandbox",
    about = "Launch Claude Code in a sandboxed Apple container VM.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser)]
enum Commands {
    /// Initialize workspace with default Containerfile
    Init {
        /// Overwrite existing files in .claude-sandbox/
        #[arg(long)]
        force: bool,

        /// Container image name (default: claude-sandbox-<dirname>)
        #[arg(long)]
        name: Option<String>,
    },

    /// Build the sandbox container image
    Build,

    /// Run Claude Code in the container
    Run {
        /// Number of CPUs (2-8)
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(2..=8))]
        cpus: u8,

        /// Memory in GB (2-8)
        #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u8).range(2..=8))]
        memory: u8,
    },
}

fn main() -> Result<()> {
    env_logger::Builder::from_default_env()
        .format(|buf, record| writeln!(buf, "{} {}", record.level(), record.args()))
        .init();

    match Cli::parse().command {
        Commands::Init { force, name } => cmd_init(force, name.as_deref()),
        Commands::Build => cmd_build(),
        Commands::Run { cpus, memory } => cmd_run(cpus, memory),
    }
}

fn cmd_init(force: bool, name: Option<&str>) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let sandbox_dir = cwd.join(SANDBOX_DIR);

    let image = match name {
        Some(n) => n.to_string(),
        None => default_image_name(&cwd)?,
    };

    init_sandbox(&sandbox_dir, force, &image)
}

fn cmd_run(cpus: u8, memory: u8) -> Result<()> {
    check_container_available()?;

    let cwd = env::current_dir().context("failed to get current directory")?;
    let sandbox_dir = cwd.join(SANDBOX_DIR);
    let image = read_image_name(&sandbox_dir)?;
    check_image_built(&image)?;

    let settings_path = sandbox_dir.join("settings.json");
    if !settings_path.exists() {
        bail!(".claude-sandbox/settings.json not found.\n{HINT_RUN_INIT}");
    }

    debug!("reading keychain service: {}", KEYCHAIN_SERVICE);
    let json_str = exec_output_quiet(
        "security",
        &["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"],
    )
    .filter(|o| o.status.success())
    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    .filter(|s| !s.is_empty())
    .context(
        "No OAuth token found in keychain.\n\n\
             Please authenticate using the official Claude CLI first:\n  \
             claude auth login",
    )?;

    let creds: serde_json::Value =
        serde_json::from_str(&json_str).context("Failed to parse keychain credentials as JSON")?;

    let token = creds["claudeAiOauth"]["accessToken"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .context("No accessToken found in keychain credentials")?;

    debug!(
        "running image '{}' with cpus={}, memory={}G",
        image, cpus, memory
    );

    let code_volume = format!("{}:/home/claude/code", cwd.display());
    let settings_volume = format!(
        "{}:/home/claude/.claude/settings.json",
        settings_path.display()
    );

    let args = vec![
        "run".to_string(),
        "--rm".to_string(),
        "-it".to_string(),
        "-e".to_string(),
        "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
        "-m".to_string(),
        format!("{}G", memory),
        "-c".to_string(),
        cpus.to_string(),
        "-v".to_string(),
        code_volume,
        "-v".to_string(),
        settings_volume,
        image,
    ];

    debug!(
        "exec: container run {} (token redacted)",
        args[1..].join(" ")
    );

    let err = Command::new("container")
        .args(&args)
        .env("CLAUDE_CODE_OAUTH_TOKEN", token)
        .exec();

    Err(anyhow::anyhow!(err).context("failed to exec container run"))
}

fn init_sandbox(sandbox_dir: &Path, force: bool, image: &str) -> Result<()> {
    if !force && sandbox_dir.join("Containerfile").exists() {
        bail!(".claude-sandbox already initialized. Use --force to overwrite.");
    }

    fs::create_dir_all(sandbox_dir).context("failed to create .claude-sandbox directory")?;

    fs::write(sandbox_dir.join(IMAGE_NAME_FILE), format!("{}\n", image))
        .context("failed to write .claude-sandbox/image-name")?;

    for (name, content) in [
        ("Containerfile", include_str!("../assets/Containerfile")),
        ("claude.json", include_str!("../assets/claude.json")),
        ("settings.json", include_str!("../assets/settings.json")),
        ("CLAUDE.md", include_str!("../assets/CLAUDE.md")),
        (".gitconfig", include_str!("../assets/.gitconfig")),
        ("sandbox-test.sh", include_str!("../assets/sandbox-test.sh")),
    ] {
        fs::write(sandbox_dir.join(name), content)
            .with_context(|| format!("failed to write .claude-sandbox/{name}"))?;
    }

    let hooks_dir = sandbox_dir.join("git-hooks");
    fs::create_dir_all(&hooks_dir).context("failed to create .claude-sandbox/git-hooks")?;
    for (name, content) in [
        ("pre-commit", include_str!("../assets/git-hooks/pre-commit")),
        ("pre-push", include_str!("../assets/git-hooks/pre-push")),
    ] {
        fs::write(hooks_dir.join(name), content)
            .with_context(|| format!("failed to write .claude-sandbox/git-hooks/{name}"))?;
    }

    info!(
        "Initialized workspace in .claude-sandbox/ (image: {})",
        image
    );
    Ok(())
}

fn cmd_build() -> Result<()> {
    check_container_available()?;

    let cwd = env::current_dir().context("failed to get current directory")?;
    let sandbox_dir = cwd.join(SANDBOX_DIR);
    let image = read_image_name(&sandbox_dir)?;

    if !sandbox_dir.join("Containerfile").exists() {
        bail!(".claude-sandbox/Containerfile not found.\n{HINT_RUN_INIT}");
    }

    let sandbox_str = sandbox_dir.to_str().context("invalid sandbox path")?;
    let containerfile_path = sandbox_dir.join("Containerfile");
    let containerfile_str = containerfile_path
        .to_str()
        .context("invalid Containerfile path")?;

    info!("Building image '{}'...", image);

    let status = Command::new("container")
        .args(["build", "-t", &image, "-f", containerfile_str, sandbox_str])
        .status()
        .context("failed to execute: container")?;

    if !status.success() {
        bail!("container build failed for '{}'", image);
    }

    info!("Image '{}' built successfully", image);
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn default_image_name(cwd: &Path) -> Result<String> {
    let dir_name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .context("failed to determine project directory name")?;
    let sanitized: String = dir_name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    Ok(format!("{}-{}", IMAGE_PREFIX, sanitized))
}

fn read_image_name(sandbox_dir: &Path) -> Result<String> {
    let path = sandbox_dir.join(IMAGE_NAME_FILE);
    let content = fs::read_to_string(&path).with_context(|| {
        format!(
            ".claude-sandbox/{} not found.\n{}",
            IMAGE_NAME_FILE, HINT_RUN_INIT
        )
    })?;
    let name = content.trim().to_string();
    if name.is_empty() {
        bail!(".claude-sandbox/{} is empty", IMAGE_NAME_FILE);
    }
    Ok(name)
}

fn check_container_available() -> Result<()> {
    debug!("checking container CLI availability");
    if exec_output_quiet("container", &["--version"]).is_none() {
        bail!(
            "Apple container CLI not found.\n\n\
             Install it from: https://developer.apple.com/documentation/virtualization"
        );
    }
    debug!("container CLI available");
    Ok(())
}

fn check_image_built(image: &str) -> Result<()> {
    debug!("checking image '{}' exists locally", image);
    let exists = exec_output_quiet("container", &["image", "inspect", image])
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exists {
        bail!(
            "Image '{}' not found locally.\n\n\
             Run 'claude-sandbox build' to build it first.",
            image
        );
    }
    Ok(())
}

fn exec_output_quiet(program: &str, args: &[&str]) -> Option<Output> {
    Command::new(program).args(args).output().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = dir.path().join(".claude-sandbox");
        init_sandbox(&sandbox, false, "claude-sandbox-test").unwrap();
        assert!(sandbox.join("Containerfile").exists());
        assert!(sandbox.join("claude.json").exists());
        assert!(sandbox.join("settings.json").exists());
        assert!(sandbox.join("CLAUDE.md").exists());
        assert!(sandbox.join("sandbox-test.sh").exists());
        assert!(sandbox.join("git-hooks/pre-commit").exists());
        assert!(sandbox.join("git-hooks/pre-push").exists());
    }

    #[test]
    fn test_init_writes_image_name() {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = dir.path().join(".claude-sandbox");
        init_sandbox(&sandbox, false, "claude-sandbox-myapp").unwrap();
        let name = read_image_name(&sandbox).unwrap();
        assert_eq!(name, "claude-sandbox-myapp");
    }

    #[test]
    fn test_init_custom_name() {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = dir.path().join(".claude-sandbox");
        init_sandbox(&sandbox, false, "my-custom-image").unwrap();
        let name = read_image_name(&sandbox).unwrap();
        assert_eq!(name, "my-custom-image");
    }

    #[test]
    fn test_default_image_name() {
        let name = default_image_name(Path::new("/Users/me/my-project")).unwrap();
        assert_eq!(name, "claude-sandbox-my-project");
    }

    #[test]
    fn test_default_image_name_sanitizes() {
        let name = default_image_name(Path::new("/Users/me/My Project_v2")).unwrap();
        assert_eq!(name, "claude-sandbox-my-project-v2");
    }

    #[test]
    fn test_init_refuses_if_already_initialized() {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = dir.path().join(".claude-sandbox");
        init_sandbox(&sandbox, false, "claude-sandbox-test").unwrap();
        assert!(init_sandbox(&sandbox, false, "claude-sandbox-test").is_err());
    }

    #[test]
    fn test_init_force_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = dir.path().join(".claude-sandbox");
        init_sandbox(&sandbox, false, "claude-sandbox-test").unwrap();
        fs::write(sandbox.join("Containerfile"), b"modified").unwrap();
        init_sandbox(&sandbox, true, "claude-sandbox-test").unwrap();
        assert_eq!(
            fs::read_to_string(sandbox.join("Containerfile")).unwrap(),
            include_str!("../assets/Containerfile")
        );
    }

    #[test]
    fn test_read_image_name_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_image_name(dir.path()).is_err());
    }
}
