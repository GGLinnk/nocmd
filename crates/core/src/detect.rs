//! Environment detection used to conditionally enable redirection packs.
//!
//! Two signals are gathered, both read fresh on each invocation (a PreToolUse
//! hook is a short-lived process, so there is nothing to cache across calls):
//!
//!   * **MCP servers** — the *configured* server names found in the standard
//!     Claude Code config files. A hook cannot enumerate the live tool list,
//!     but it can read which servers a project/user has declared and match a
//!     pack against them (e.g. a server whose name contains `cargo`).
//!   * **PATH commands** — whether a redirect *target* (e.g. `bun`, `uv`) is
//!     actually installed, so we only nudge toward tools that exist.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Files larger than this are skipped when scanning for MCP servers. `~/.claude.json`
/// can grow large with history; this keeps the per-call cost bounded.
const MAX_CONFIG_BYTES: u64 = 16 * 1024 * 1024;

/// A snapshot of the environment used to resolve pack `requires` conditions.
pub struct Detect {
    /// Lowercased names of configured MCP servers.
    mcp: BTreeSet<String>,
    /// Directories on `PATH`.
    path_dirs: Vec<PathBuf>,
    /// Executable extensions to try (lowercased; `[""]` on Unix).
    exe_exts: Vec<String>,
}

impl Detect {
    /// Build a detection snapshot rooted at `cwd`.
    pub fn new(cwd: &Path) -> Self {
        Detect {
            mcp: scan_mcp_servers(cwd),
            path_dirs: path_dirs(),
            exe_exts: exe_exts(),
        }
    }

    /// An empty snapshot (nothing detected) — handy for tests and as a fallback.
    pub fn empty() -> Self {
        Detect {
            mcp: BTreeSet::new(),
            path_dirs: Vec::new(),
            exe_exts: vec![String::new()],
        }
    }

    /// The configured MCP server name that *contains* `needle` (case-insensitive),
    /// if any. Used both to decide enablement and to expand `{mcp}`.
    pub fn matched_mcp(&self, needle: &str) -> Option<&str> {
        let n = needle.to_ascii_lowercase();
        self.mcp.iter().find(|s| s.contains(&n)).map(String::as_str)
    }

    /// Whether any configured MCP server name contains `needle`.
    pub fn has_mcp(&self, needle: &str) -> bool {
        self.matched_mcp(needle).is_some()
    }

    /// Whether `name` resolves to an executable on `PATH`.
    pub fn has_command(&self, name: &str) -> bool {
        for dir in &self.path_dirs {
            for ext in &self.exe_exts {
                let mut file = name.to_string();
                file.push_str(ext);
                if dir.join(&file).is_file() {
                    return true;
                }
            }
        }
        false
    }

    /// Iterate the detected MCP server names (sorted).
    pub fn mcp_servers(&self) -> impl Iterator<Item = &str> {
        self.mcp.iter().map(String::as_str)
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default()
}

fn exe_exts() -> Vec<String> {
    if cfg!(windows) {
        let mut v: Vec<String> = std::env::var("PATHEXT")
            .ok()
            .map(|p| {
                p.split(';')
                    .filter(|e| !e.is_empty())
                    .map(|e| e.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_else(|| vec![".exe".into(), ".cmd".into(), ".bat".into(), ".com".into()]);
        // Also try the bare name in case it already carries an extension.
        v.push(String::new());
        v
    } else {
        vec![String::new()]
    }
}

/// Normalize a path string for cross-platform comparison.
fn norm_path(p: &str) -> String {
    p.replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

/// Add the `mcpServers` keys and `enabledMcpjsonServers` entries from a JSON
/// object to `set`.
fn add_servers(v: &serde_json::Value, set: &mut BTreeSet<String>) {
    if let Some(obj) = v.get("mcpServers").and_then(|x| x.as_object()) {
        for k in obj.keys() {
            set.insert(k.to_ascii_lowercase());
        }
    }
    if let Some(arr) = v.get("enabledMcpjsonServers").and_then(|x| x.as_array()) {
        for s in arr {
            if let Some(name) = s.as_str() {
                set.insert(name.to_ascii_lowercase());
            }
        }
    }
}

fn read_capped(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_CONFIG_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn collect_file(path: &Path, set: &mut BTreeSet<String>) {
    if let Some(txt) = read_capped(path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
            add_servers(&v, set);
        }
    }
}

/// Scan the standard Claude Code config locations for configured MCP servers.
fn scan_mcp_servers(cwd: &Path) -> BTreeSet<String> {
    let mut set = BTreeSet::new();

    // Project files: walk up from cwd to the filesystem root.
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        collect_file(&d.join(".mcp.json"), &mut set);
        collect_file(&d.join(".claude").join("settings.json"), &mut set);
        collect_file(&d.join(".claude").join("settings.local.json"), &mut set);
        dir = d.parent();
    }

    // User-level files.
    if let Some(home) = home_dir() {
        collect_file(&home.join(".claude").join("settings.json"), &mut set);
        if let Some(txt) = read_capped(&home.join(".claude.json")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                add_servers(&v, &mut set);
                // Per-project server lists keyed by absolute path.
                if let Some(projects) = v.get("projects").and_then(|x| x.as_object()) {
                    let target = norm_path(&cwd.to_string_lossy());
                    for (k, pv) in projects {
                        if norm_path(k) == target {
                            add_servers(pv, &mut set);
                        }
                    }
                }
            }
        }
    }

    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_servers_reads_both_shapes() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{
                "mcpServers": { "cargo-mcp": {}, "bun-mcp": {} },
                "enabledMcpjsonServers": ["git-tools"]
            }"#,
        )
        .unwrap();
        let mut set = BTreeSet::new();
        add_servers(&v, &mut set);
        assert!(set.contains("cargo-mcp"));
        assert!(set.contains("bun-mcp"));
        assert!(set.contains("git-tools"));
    }

    #[test]
    fn matched_mcp_is_substring() {
        let mut mcp = BTreeSet::new();
        mcp.insert("cargo-mcp".to_string());
        let d = Detect {
            mcp,
            path_dirs: Vec::new(),
            exe_exts: vec![String::new()],
        };
        assert_eq!(d.matched_mcp("cargo"), Some("cargo-mcp"));
        assert!(!d.has_mcp("git"));
    }
}
