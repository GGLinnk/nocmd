#![forbid(unsafe_code)]
//! Workspace packaging tasks for the nocmd Claude Code plugin.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Parser, Subcommand};

/// Name of the plugin, its binary, and the bundle directory.
const PLUGIN_NAME: &str = "nocmd";

#[derive(Parser)]
#[command(name = "xtask", about = "nocmd workspace tasks")]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Build the release binary and bundle the plugin into <out>/nocmd-<target>.zip.
    Package {
        /// Target triple to build for (defaults to the host toolchain).
        #[arg(long, env = "NOCMD_XTASK_TARGET")]
        target: Option<String>,
        /// Output directory for the bundle and the zip.
        #[arg(long, env = "NOCMD_XTASK_OUT", default_value = "dist")]
        out: PathBuf,
    },
}

type Fallible = Result<(), Box<dyn std::error::Error>>;

fn main() -> Fallible {
    match Cli::parse().task {
        Task::Package { target, out } => package(target.as_deref(), &out),
    }
}

/// The workspace root is the parent of this crate's manifest directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives under the workspace root")
        .to_path_buf()
}

/// Executable suffix for the bundle's binary: Windows targets get `.exe`,
/// other explicit targets get none, and an unset target follows the host.
fn exe_suffix(target: Option<&str>) -> &'static str {
    match target {
        Some(t) if t.contains("windows") => ".exe",
        Some(_) => "",
        None => std::env::consts::EXE_SUFFIX,
    }
}

fn package(target: Option<&str>, out: &Path) -> Fallible {
    let root = workspace_root();
    let suffix = exe_suffix(target);
    let binary = format!("{PLUGIN_NAME}{suffix}");

    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(&root)
        .args(["build", "--release", "--package", PLUGIN_NAME]);
    if let Some(t) = target {
        build.args(["--target", t]);
    }
    if !build.status()?.success() {
        return Err("cargo build --release failed".into());
    }

    let mut bin_src = root.join("target");
    if let Some(t) = target {
        bin_src.push(t);
    }
    bin_src.push("release");
    bin_src.push(&binary);

    let bundle = out.join(PLUGIN_NAME);
    if bundle.exists() {
        fs::remove_dir_all(&bundle)?;
    }
    fs::create_dir_all(bundle.join(".claude-plugin"))?;
    fs::create_dir_all(bundle.join("hooks"))?;
    fs::create_dir_all(bundle.join("bin"))?;

    fs::copy(
        root.join(".claude-plugin").join("plugin.json"),
        bundle.join(".claude-plugin").join("plugin.json"),
    )?;
    fs::write(bundle.join("hooks").join("hooks.json"), hooks_json(&binary))?;
    fs::copy(&bin_src, bundle.join("bin").join(&binary))?;

    let label = target.unwrap_or("host");
    let zip_path = out.join(format!("{PLUGIN_NAME}-{label}.zip"));
    zip_dir(&bundle, &zip_path)?;

    println!("packaged {} -> {}", bundle.display(), zip_path.display());
    Ok(())
}

/// Render `hooks.json` pointing at the bundled binary (name carries the target's
/// executable suffix, so Windows bundles reference `nocmd.exe`).
fn hooks_json(binary: &str) -> String {
    const TEMPLATE: &str = r#"{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "${CLAUDE_PLUGIN_ROOT}/bin/__BIN__" }
        ]
      }
    ]
  }
}
"#;
    TEMPLATE.replace("__BIN__", binary)
}

fn zip_dir(src: &Path, zip_path: &Path) -> Fallible {
    let file = fs::File::create(zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    add_dir(&mut zip, src, src, opts)?;
    zip.finish()?;
    Ok(())
}

fn add_dir<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    base: &Path,
    dir: &Path,
    opts: zip::write::SimpleFileOptions,
) -> Fallible {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let rel = path
            .strip_prefix(base)?
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            zip.add_directory(format!("{rel}/"), opts)?;
            add_dir(zip, base, &path, opts)?;
        } else {
            zip.start_file(rel, opts)?;
            zip.write_all(&fs::read(&path)?)?;
        }
    }
    Ok(())
}
