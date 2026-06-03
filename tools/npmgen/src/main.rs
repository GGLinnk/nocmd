#![forbid(unsafe_code)]
//! Generate the npm publish tree for the nocmd plugin: the meta package plus
//! one package per platform. Everything identifying the package is read from
//! Cargo.toml so it stays the single source of truth: version from
//! `CARGO_PKG_VERSION`, author from `CARGO_PKG_AUTHORS`, description from
//! `CARGO_PKG_DESCRIPTION`, and the npm scope, plugin name, and git URL all
//! derived from `CARGO_PKG_REPOSITORY`. The supported platforms live in
//! `TARGETS`.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde_json::json;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

/// A supported platform: the npm key (`<process.platform>-<process.arch>`) plus
/// the npm `os`/`cpu` install filters.
struct Target {
    key: &'static str,
    os: &'static str,
    cpu: &'static str,
    windows: bool,
}

const TARGETS: &[Target] = &[
    Target {
        key: "win32-x64",
        os: "win32",
        cpu: "x64",
        windows: true,
    },
    Target {
        key: "win32-arm64",
        os: "win32",
        cpu: "arm64",
        windows: true,
    },
    Target {
        key: "darwin-x64",
        os: "darwin",
        cpu: "x64",
        windows: false,
    },
    Target {
        key: "darwin-arm64",
        os: "darwin",
        cpu: "arm64",
        windows: false,
    },
    Target {
        key: "linux-x64",
        os: "linux",
        cpu: "x64",
        windows: false,
    },
    Target {
        key: "linux-arm64",
        os: "linux",
        cpu: "arm64",
        windows: false,
    },
];

/// Package identity derived from the repository URL: the npm scope (`@owner`),
/// the plugin and binary name (repo basename), and the npm git URL. Assumes the
/// npm scope matches the repository owner.
struct Identity {
    scope: String,
    plugin: &'static str,
    git_url: String,
}

/// Failures while generating the npm tree. Each variant names the offending
/// path and chains the underlying cause.
#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("[workspace.package] repository must be set to https://<host>/<owner>/<repo>")]
    MissingRepository,

    #[error("git tag {tag} does not match the workspace version {expected}")]
    TagMismatch { tag: String, expected: String },

    #[error("creating directory {}: {source}", path.display())]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("serializing JSON for {}: {source}", path.display())]
    Serialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("writing {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("copying {} to {}: {source}", from.display(), to.display())]
    Copy {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

type Result<T> = std::result::Result<T, Error>;

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

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    if let Err(error) = run(Cli::parse()) {
        tracing::error!(%error, "npmgen failed");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    if let Some(tag) = &cli.tag {
        let expected = format!("v{VERSION}");
        if tag != &expected {
            return Err(Error::TagMismatch {
                tag: tag.clone(),
                expected,
            });
        }
        debug!(tag, version = VERSION, "tag matches workspace version");
    }

    let id = identity()?;
    let root = workspace_root();
    write_meta(&cli.out.join(id.plugin), &root, &id)?;
    for target in TARGETS {
        write_platform(&cli.out, target, &id)?;
    }

    info!(
        package = %format!("{}/{}", id.scope, id.plugin),
        version = VERSION,
        targets = TARGETS.len(),
        out = %cli.out.display(),
        "generated npm publish tree",
    );
    Ok(())
}

/// Derive the package identity from `CARGO_PKG_REPOSITORY`.
fn identity() -> Result<Identity> {
    let path = REPOSITORY.trim_end_matches('/').trim_end_matches(".git");
    let mut segments = path.rsplit('/');
    let plugin = segments.next().unwrap_or_default();
    let owner = segments.next().unwrap_or_default();
    if owner.is_empty() || plugin.is_empty() {
        return Err(Error::MissingRepository);
    }
    Ok(Identity {
        scope: format!("@{owner}"),
        plugin,
        git_url: format!("git+{path}.git"),
    })
}

/// First `CARGO_PKG_AUTHORS` entry parsed into (display, name, optional email).
fn author() -> (&'static str, &'static str, Option<&'static str>) {
    let full = AUTHORS.split(':').next().unwrap_or(AUTHORS).trim();
    match full.split_once('<') {
        Some((name, email)) => (full, name.trim(), Some(email.trim_end_matches('>').trim())),
        None => (full, full, None),
    }
}

/// `tools/npmgen` -> `tools` -> workspace root.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("npmgen lives at tools/npmgen under the workspace root")
        .to_path_buf()
}

fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| Error::CreateDir {
        path: path.to_path_buf(),
        source,
    })
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_dir(parent)?;
    }
    let mut text = serde_json::to_string_pretty(value).map_err(|source| Error::Serialize {
        path: path.to_path_buf(),
        source,
    })?;
    text.push('\n');
    fs::write(path, text).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;
    debug!(path = %path.display(), "wrote config");
    Ok(())
}

fn copy_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        create_dir(parent)?;
    }
    fs::copy(from, to).map_err(|source| Error::Copy {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        source,
    })?;
    debug!(from = %from.display(), to = %to.display(), "copied source file");
    Ok(())
}

fn write_meta(dir: &Path, root: &Path, id: &Identity) -> Result<()> {
    create_dir(dir)?;

    let (author_full, author_name, author_email) = author();
    let plugin_author = match author_email {
        Some(email) => json!({ "name": author_name, "email": email }),
        None => json!({ "name": author_name }),
    };

    let optional: serde_json::Map<String, serde_json::Value> = TARGETS
        .iter()
        .map(|t| {
            (
                format!("{}/{}-{}", id.scope, id.plugin, t.key),
                json!(VERSION),
            )
        })
        .collect();

    write_json(
        &dir.join("package.json"),
        &json!({
            "name": format!("{}/{}", id.scope, id.plugin),
            "version": VERSION,
            "description": DESCRIPTION,
            "license": "MIT",
            "author": author_full,
            "repository": { "type": "git", "url": id.git_url.as_str() },
            "files": [".claude-plugin", "hooks", "launch.mjs"],
            "optionalDependencies": optional,
            "publishConfig": { "access": "public" },
        }),
    )?;

    write_json(
        &dir.join(".claude-plugin").join("plugin.json"),
        &json!({
            "name": id.plugin,
            "version": VERSION,
            "description": DESCRIPTION,
            "author": plugin_author,
            "license": "MIT",
            "keywords": ["hook", "pretooluse", "bash", "mcp", "guard"],
        }),
    )?;

    copy_file(&root.join("launch.mjs"), &dir.join("launch.mjs"))?;
    copy_file(
        &root.join("hooks").join("hooks.json"),
        &dir.join("hooks").join("hooks.json"),
    )?;
    info!(dir = %dir.display(), "wrote meta package");
    Ok(())
}

fn write_platform(out: &Path, target: &Target, id: &Identity) -> Result<()> {
    let dir = out.join(format!("{}-{}", id.plugin, target.key));
    let ext = if target.windows { ".exe" } else { "" };
    let binary = format!("{}{ext}", id.plugin);
    write_json(
        &dir.join("package.json"),
        &json!({
            "name": format!("{}/{}-{}", id.scope, id.plugin, target.key),
            "version": VERSION,
            "description": format!("{} binary for {}.", id.plugin, target.key),
            "license": "MIT",
            "os": [target.os],
            "cpu": [target.cpu],
            "files": [binary],
        }),
    )?;
    debug!(target = target.key, "wrote platform package");
    Ok(())
}
