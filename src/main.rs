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
const DEFAULT_IMAGE_NAME: &str = "claude-sandbox";
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

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
    },

    /// Build container image from Containerfile in current directory
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
        Commands::Init { force } => cmd_init(force),
        Commands::Build => cmd_build(),
        Commands::Run { cpus, memory } => cmd_run(cpus, memory),
    }
}

fn cmd_init(force: bool) -> Result<()> {
    let sandbox_dir = env::current_dir()
        .context("failed to get current directory")?
        .join(SANDBOX_DIR);
    init_sandbox(&sandbox_dir, force)
}

fn cmd_run(cpus: u8, memory: u8) -> Result<()> {
    check_container_available()?;
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

    debug!("running with cpus={}, memory={}G", cpus, memory);

    let volume = format!(
        "{}:/home/claude/code",
        env::current_dir()
            .context("failed to determine working directory")?
            .display()
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
        volume,
        DEFAULT_IMAGE_NAME.to_string(),
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

fn init_sandbox(sandbox_dir: &Path, force: bool) -> Result<()> {
    if !force && sandbox_dir.join("Containerfile").exists() {
        bail!(".claude-sandbox already initialized. Use --force to overwrite.");
    }

    fs::create_dir_all(sandbox_dir).context("failed to create .claude-sandbox directory")?;

    for (name, content) in [
        ("Containerfile", include_str!("sandbox/Containerfile")),
        ("claude.json", include_str!("sandbox/claude.json")),
        ("settings.json", include_str!("sandbox/settings.json")),
        ("CLAUDE.md", include_str!("sandbox/CLAUDE.md")),
    ] {
        fs::write(sandbox_dir.join(name), content)
            .with_context(|| format!("failed to write .claude-sandbox/{name}"))?;
    }

    info!("Initialized workspace in .claude-sandbox/");
    Ok(())
}

fn cmd_build() -> Result<()> {
    check_container_available()?;
    let cwd = env::current_dir().context("failed to get current directory")?;
    let sandbox_dir = cwd.join(SANDBOX_DIR);
    let containerfile_path = sandbox_dir.join("Containerfile");

    if !containerfile_path.exists() {
        bail!(
            ".claude-sandbox/Containerfile not found.\n\
             Run 'claude-sandbox init' first to initialize the workspace."
        );
    }

    let containerfile_str = containerfile_path
        .to_str()
        .context("invalid Containerfile path")?;
    let sandbox_str = sandbox_dir.to_str().context("invalid sandbox path")?;

    debug!(
        "building image '{}' from {}",
        DEFAULT_IMAGE_NAME, containerfile_str
    );

    let status = Command::new("container")
        .args([
            "build",
            "-t",
            DEFAULT_IMAGE_NAME,
            "-f",
            containerfile_str,
            sandbox_str,
        ])
        .status()
        .context("failed to execute: container")?;

    if !status.success() {
        bail!("container build failed");
    }

    info!("Image '{}' built successfully", DEFAULT_IMAGE_NAME);
    Ok(())
}

fn check_container_available() -> Result<()> {
    debug!("checking container CLI availability");
    if exec_output_quiet("container", &["--version"]).is_none() {
        bail!("Apple container CLI not found.");
    }
    debug!("container CLI available");
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
        init_sandbox(&sandbox, false).unwrap();
        assert!(sandbox.join("Containerfile").exists());
        assert!(sandbox.join("claude.json").exists());
        assert!(sandbox.join("settings.json").exists());
        assert!(sandbox.join("CLAUDE.md").exists());
    }

    #[test]
    fn test_init_refuses_if_already_initialized() {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = dir.path().join(".claude-sandbox");
        init_sandbox(&sandbox, false).unwrap();
        assert!(init_sandbox(&sandbox, false).is_err());
    }

    #[test]
    fn test_init_force_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = dir.path().join(".claude-sandbox");
        init_sandbox(&sandbox, false).unwrap();
        fs::write(sandbox.join("Containerfile"), b"modified").unwrap();
        init_sandbox(&sandbox, true).unwrap();
        assert_eq!(
            fs::read_to_string(sandbox.join("Containerfile")).unwrap(),
            include_str!("sandbox/Containerfile")
        );
    }
}
