#![forbid(unsafe_code)]
//! Generate the npm publish tree for the nocmd plugin: the meta package plus
//! one package per platform. The version comes from this crate's
//! `CARGO_PKG_VERSION` (the shared workspace version), so `Cargo.toml` is the
//! single source of truth for the version, and `TARGETS` is the single source
//! for the supported platforms.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde_json::json;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SCOPE: &str = "@gglinnk";
const PLUGIN: &str = "nocmd";
const AUTHOR: &str = "Gabriel GRONDIN";
const REPOSITORY: &str = "git+https://github.com/gglinnk/nocmd.git";
const DESCRIPTION: &str = "PreToolUse Bash hook that redirects discouraged shell commands to Claude's dedicated tools and to configured MCP servers.";

/// A supported platform: the npm key (`<process.platform>-<process.arch>`) plus
/// the npm `os`/`cpu` install filters.
struct Target {
    key: &'static str,
    os: &'static str,
    cpu: &'static str,
    windows: bool,
}

const TARGETS: &[Target] = &[
    Target { key: "win32-x64", os: "win32", cpu: "x64", windows: true },
    Target { key: "win32-arm64", os: "win32", cpu: "arm64", windows: true },
    Target { key: "darwin-x64", os: "darwin", cpu: "x64", windows: false },
    Target { key: "darwin-arm64", os: "darwin", cpu: "arm64", windows: false },
    Target { key: "linux-x64", os: "linux", cpu: "x64", windows: false },
    Target { key: "linux-arm64", os: "linux", cpu: "arm64", windows: false },
];

#[derive(Parser)]
#[command(name = "npmgen", about = "Generate the nocmd npm publish tree")]
struct Cli {
    /// Output directory for the generated npm package tree.
    #[arg(long, env = "NPMGEN_OUT", default_value = "dist/npm")]
    out: PathBuf,
    /// When set, require this git tag to equal `v<workspace-version>`.
    #[arg(long, env = "NPMGEN_TAG")]
    tag: Option<String>,
}

type Fallible = Result<(), Box<dyn std::error::Error>>;

fn main() -> Fallible {
    let cli = Cli::parse();

    if let Some(tag) = &cli.tag {
        let expected = format!("v{VERSION}");
        if tag != &expected {
            return Err(format!("tag {tag} does not match workspace version {expected}").into());
        }
    }

    let root = workspace_root();
    write_meta(&cli.out.join(PLUGIN), &root)?;
    for target in TARGETS {
        write_platform(&cli.out, target)?;
    }

    println!("generated {SCOPE}/{PLUGIN} {VERSION} ({} targets) in {}", TARGETS.len(), cli.out.display());
    Ok(())
}

/// `tools/npmgen` -> `tools` -> workspace root.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("npmgen lives at tools/npmgen under the workspace root")
        .to_path_buf()
}

fn write_json(path: &Path, value: &serde_json::Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    fs::write(path, text)
}

fn write_meta(dir: &Path, root: &Path) -> Fallible {
    fs::create_dir_all(dir)?;

    let optional: serde_json::Map<String, serde_json::Value> = TARGETS
        .iter()
        .map(|t| (format!("{SCOPE}/{PLUGIN}-{}", t.key), json!(VERSION)))
        .collect();

    let package = json!({
        "name": format!("{SCOPE}/{PLUGIN}"),
        "version": VERSION,
        "description": DESCRIPTION,
        "license": "MIT",
        "author": AUTHOR,
        "repository": { "type": "git", "url": REPOSITORY },
        "files": [".claude-plugin", "hooks", "launch.mjs"],
        "optionalDependencies": optional,
        "publishConfig": { "access": "public" },
    });
    write_json(&dir.join("package.json"), &package)?;

    let plugin = json!({
        "name": PLUGIN,
        "version": VERSION,
        "description": DESCRIPTION,
        "author": { "name": AUTHOR },
        "license": "MIT",
        "keywords": ["hook", "pretooluse", "bash", "mcp", "guard"],
    });
    write_json(&dir.join(".claude-plugin").join("plugin.json"), &plugin)?;

    // Source files shipped verbatim inside the meta package.
    fs::copy(root.join("launch.mjs"), dir.join("launch.mjs"))?;
    fs::create_dir_all(dir.join("hooks"))?;
    fs::copy(
        root.join("hooks").join("hooks.json"),
        dir.join("hooks").join("hooks.json"),
    )?;
    Ok(())
}

fn write_platform(out: &Path, target: &Target) -> std::io::Result<()> {
    let dir = out.join(format!("{PLUGIN}-{}", target.key));
    fs::create_dir_all(&dir)?;
    let binary = if target.windows { "nocmd.exe" } else { "nocmd" };
    let package = json!({
        "name": format!("{SCOPE}/{PLUGIN}-{}", target.key),
        "version": VERSION,
        "description": format!("nocmd binary for {}.", target.key),
        "license": "MIT",
        "os": [target.os],
        "cpu": [target.cpu],
        "files": [binary],
    });
    write_json(&dir.join("package.json"), &package)
}
