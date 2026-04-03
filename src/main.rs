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

const ASSETS: &[(&str, &str)] = &[
    ("Containerfile", include_str!("assets/Containerfile")),
    ("claude.json", include_str!("assets/claude.json")),
    ("settings.json", include_str!("assets/settings.json")),
    ("CLAUDE.md", include_str!("assets/CLAUDE.md")),
    ("sandbox-tools.py", include_str!("assets/sandbox-tools.py")),
    (
        "sandbox-checks.py",
        include_str!("assets/sandbox-checks.py"),
    ),
    (".gitconfig", include_str!("assets/.gitconfig")),
];

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

    /// Compare .claude-sandbox/ files against the current binary's embedded assets
    Status,
}

fn main() -> Result<()> {
    env_logger::Builder::from_default_env()
        .format(|buf, record| writeln!(buf, "{} {}", record.level(), record.args()))
        .init();

    match Cli::parse().command {
        Commands::Init { force, name } => cmd_init(force, name.as_deref()),
        Commands::Build => cmd_build(),
        Commands::Run { cpus, memory } => cmd_run(cpus, memory),
        Commands::Status => cmd_status(),
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

    let args = run_args(&cwd, &image, cpus, memory);

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

    for (name, content) in ASSETS {
        fs::write(sandbox_dir.join(name), content)
            .with_context(|| format!("failed to write .claude-sandbox/{name}"))?;
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

fn cmd_status() -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let sandbox_dir = cwd.join(SANDBOX_DIR);

    if !sandbox_dir.join("Containerfile").exists() {
        bail!(".claude-sandbox/ not initialized.\n{HINT_RUN_INIT}");
    }

    let mut diffs = 0usize;
    for (name, embedded) in ASSETS {
        let path = sandbox_dir.join(name);
        match fs::read_to_string(&path) {
            Ok(ref content) if content == embedded => println!("  ok    {name}"),
            Ok(_) => {
                println!("  DIFF  {name}");
                diffs += 1;
            }
            Err(_) => {
                println!("  MISS  {name}");
                diffs += 1;
            }
        }
    }

    if diffs > 0 {
        println!("\n{diffs} file(s) differ — run 'claude-sandbox init --force' to update");
    } else {
        println!("\nAll files match");
    }

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

fn run_args(cwd: &Path, image: &str, cpus: u8, memory: u8) -> Vec<String> {
    let code_volume = format!("{}:/home/claude/code", cwd.display());
    vec![
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
        image.to_string(),
    ]
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
        assert!(sandbox.join("sandbox-tools.py").exists());
        assert!(sandbox.join("sandbox-checks.py").exists());
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
            include_str!("assets/Containerfile")
        );
    }

    #[test]
    fn test_assets_match_after_init() {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = dir.path().join(".claude-sandbox");
        init_sandbox(&sandbox, false, "test").unwrap();
        for (name, embedded) in ASSETS {
            let content = fs::read_to_string(sandbox.join(name)).unwrap();
            assert_eq!(&content, embedded, "{name} differs after init");
        }
    }

    #[test]
    fn test_assets_detect_modification() {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = dir.path().join(".claude-sandbox");
        init_sandbox(&sandbox, false, "test").unwrap();
        fs::write(sandbox.join("Containerfile"), "modified").unwrap();
        let content = fs::read_to_string(sandbox.join("Containerfile")).unwrap();
        assert_ne!(content, include_str!("assets/Containerfile"));
    }

    #[test]
    fn test_read_image_name_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_image_name(dir.path()).is_err());
    }

    #[test]
    fn test_run_args_default() {
        let args = run_args(
            Path::new("/Users/me/project"),
            "claude-sandbox-project",
            2,
            4,
        );
        assert_eq!(
            args,
            vec![
                "run",
                "--rm",
                "-it",
                "-e",
                "CLAUDE_CODE_OAUTH_TOKEN",
                "-m",
                "4G",
                "-c",
                "2",
                "-v",
                "/Users/me/project:/home/claude/code",
                "claude-sandbox-project",
            ]
        );
    }

    #[test]
    fn test_run_args_custom_resources() {
        let args = run_args(Path::new("/tmp/test"), "my-image", 8, 8);
        assert_eq!(args[5], "-m");
        assert_eq!(args[6], "8G");
        assert_eq!(args[7], "-c");
        assert_eq!(args[8], "8");
    }

    #[test]
    fn test_run_args_volume_mount() {
        let args = run_args(Path::new("/Users/me/my project"), "img", 2, 4);
        assert_eq!(args[10], "/Users/me/my project:/home/claude/code");
    }

    #[test]
    fn test_run_args_image_is_last() {
        let args = run_args(Path::new("/tmp"), "my-image", 2, 4);
        assert_eq!(args.last().unwrap(), "my-image");
    }
}
