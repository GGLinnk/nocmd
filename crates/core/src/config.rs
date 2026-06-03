//! Group configuration: the built-in defaults plus `.nocmd/*.toml` overrides.
//!
//! A **group** is a named set of command patterns that share a `requires`
//! condition, each mapped to a [`Redirect`]. Groups with a `requires` clause
//! are conditional "packs" (active only when an MCP server is configured and/or
//! a target command is on `PATH`).
//!
//! TOML groups describe their redirects with typed fields rather than free-form
//! messages - see [`RawGroup`] and the `.nocmd/groups.toml` example.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::redirect::{Redirect, Tool};

/// Condition under which a group (pack) is active.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Requires {
    /// A configured MCP server whose name contains this substring must exist.
    pub mcp: Option<String>,
    /// This command must resolve on `PATH`.
    pub command: Option<String>,
    /// When true, any one listed condition is sufficient (default: all required).
    #[serde(default)]
    pub any: bool,
}

/// A resolved group ready to be loaded into the engine.
#[derive(Debug, Clone)]
pub struct Group {
    pub name: String,
    /// Explicit on/off override. `None` means "decide from `requires`".
    pub enabled: Option<bool>,
    pub requires: Option<Requires>,
    /// Normalized pattern -> redirect target.
    pub commands: BTreeMap<String, Redirect>,
}

/// The full set of groups, in precedence order (later groups win ties).
#[derive(Debug, Clone)]
pub struct Config {
    pub groups: Vec<Group>,
}

/// TOML shape for a `.nocmd/*.toml` file.
#[derive(Debug, Default, Deserialize)]
pub struct RawConfig {
    #[serde(default)]
    pub groups: BTreeMap<String, RawGroup>,
}

/// TOML shape for a single `[groups.<name>]` table.
///
/// A group picks how it redirects via one or more of these fields:
///   * `tool` + `commands` - redirect each command to a built-in Claude tool.
///   * `server = true` + `commands` - redirect to the MCP server generically.
///   * `[groups.<name>.mcp]` - map each pattern to an MCP tool *suffix*.
///   * `[groups.<name>.advice]` - map each pattern to free-form guidance.
#[derive(Debug, Default, Deserialize)]
pub struct RawGroup {
    /// Built-in tool that `commands` redirect to.
    pub tool: Option<Tool>,
    /// Commands redirected to `tool` or, with `server = true`, to the MCP server.
    #[serde(default)]
    pub commands: Vec<String>,
    /// Redirect `commands` to the configured MCP server generically.
    #[serde(default)]
    pub server: bool,
    /// Pattern -> MCP tool suffix (`mcp__<server>__<suffix>`).
    #[serde(default)]
    pub mcp: BTreeMap<String, String>,
    /// Pattern -> free-form guidance.
    #[serde(default)]
    pub advice: BTreeMap<String, String>,
    pub requires: Option<Requires>,
    pub enabled: Option<bool>,
    /// Convenience inverse of `enabled`.
    pub disabled: Option<bool>,
    /// Pattern keys to remove from a (built-in) group - e.g. to re-allow `cat`.
    #[serde(default)]
    pub remove: Vec<String>,
}

/// Errors surfaced while loading `.nocmd` files. The hook itself fails open and
/// ignores these; the `check`/`detect` CLI surfaces them.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadError {
    /// A `.nocmd` file or directory could not be read.
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A `.nocmd/*.toml` file did not parse.
    #[error("{}: {source}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

/// Normalize a pattern key to lowercase, single-space-separated tokens so it
/// matches the output of [`crate::parse::command_tokens`].
pub fn normalize_pattern(key: &str) -> String {
    key.split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Turn a `RawGroup`'s typed fields into a normalized pattern -> redirect map.
fn raw_commands(rg: &RawGroup) -> BTreeMap<String, Redirect> {
    let mut commands = BTreeMap::new();
    if let Some(tool) = rg.tool {
        for c in &rg.commands {
            commands.insert(normalize_pattern(c), Redirect::Tool(tool));
        }
    }
    if rg.server {
        for c in &rg.commands {
            commands.insert(normalize_pattern(c), Redirect::McpServer);
        }
    }
    for (pat, suffix) in &rg.mcp {
        commands.insert(normalize_pattern(pat), Redirect::Mcp(suffix.clone()));
    }
    for (pat, advice) in &rg.advice {
        commands.insert(normalize_pattern(pat), Redirect::Advice(advice.clone()));
    }
    commands
}

impl Config {
    /// The built-in groups, defined one-per-file under [`crate::groups`].
    pub fn builtin() -> Self {
        Config {
            groups: crate::groups::builtins(),
        }
    }

    /// Merge a parsed TOML config on top of the current groups. Existing groups
    /// (matched by name) are updated in place; new groups are appended.
    pub fn merge_raw(&mut self, raw: RawConfig) {
        for (name, rg) in raw.groups {
            let enabled = rg.enabled.or_else(|| rg.disabled.map(|d| !d));
            let new_commands = raw_commands(&rg);

            if let Some(g) = self.groups.iter_mut().find(|g| g.name == name) {
                if let Some(e) = enabled {
                    g.enabled = Some(e);
                }
                if rg.requires.is_some() {
                    g.requires = rg.requires;
                }
                g.commands.extend(new_commands);
                for key in &rg.remove {
                    g.commands.remove(&normalize_pattern(key));
                }
            } else {
                let mut commands = new_commands;
                for key in &rg.remove {
                    commands.remove(&normalize_pattern(key));
                }
                self.groups.push(Group {
                    name,
                    enabled,
                    requires: rg.requires,
                    commands,
                });
            }
        }
    }

    /// Merge every `*.toml` file in `dir` (filename order) on top of the current
    /// groups, recording any read/parse failures in `errs`.
    pub fn merge_dir(&mut self, dir: &Path, errs: &mut Vec<LoadError>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(source) => {
                errs.push(LoadError::Io {
                    path: dir.to_path_buf(),
                    source,
                });
                return;
            }
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("toml"))
            })
            .collect();
        files.sort();
        for file in files {
            match std::fs::read_to_string(&file) {
                Ok(text) => match toml::from_str::<RawConfig>(&text) {
                    Ok(raw) => self.merge_raw(raw),
                    Err(source) => errs.push(LoadError::Parse { path: file, source }),
                },
                Err(source) => errs.push(LoadError::Io { path: file, source }),
            }
        }
    }

    /// Built-ins merged with the nearest `.nocmd` directory found by walking up
    /// from `start`. Returns the config and any load errors.
    pub fn discover_verbose(start: &Path) -> (Config, Vec<LoadError>) {
        let mut cfg = Config::builtin();
        let mut errs = Vec::new();
        if let Some(dir) = find_nocmd_dir(start) {
            cfg.merge_dir(&dir, &mut errs);
        }
        (cfg, errs)
    }

    /// Built-ins merged with `.nocmd` overrides, discarding load errors
    /// (fail-open behavior for the hook path).
    pub fn discover(start: &Path) -> Config {
        Config::discover_verbose(start).0
    }
}

/// Walk up from `start` to find the nearest `.nocmd` directory.
pub fn find_nocmd_dir(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|p| p.join(".nocmd"))
        .find(|candidate| candidate.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_tool_groups() {
        let c = Config::builtin();
        for name in ["grep", "glob", "read", "edit"] {
            assert!(c.groups.iter().any(|g| g.name == name), "missing {name}");
        }
    }

    #[test]
    fn merge_adds_advice_group() {
        let mut c = Config::builtin();
        let raw: RawConfig = toml::from_str(
            r#"
            [groups.network.advice]
            curl = "the WebFetch tool"
        "#,
        )
        .unwrap();
        c.merge_raw(raw);
        let g = c.groups.iter().find(|g| g.name == "network").unwrap();
        assert!(matches!(g.commands.get("curl"), Some(Redirect::Advice(_))));
    }

    #[test]
    fn merge_tool_group_from_toml() {
        let mut c = Config::builtin();
        let raw: RawConfig = toml::from_str(
            r#"
            [groups.pager]
            tool = "read"
            commands = ["bat", "less"]
        "#,
        )
        .unwrap();
        c.merge_raw(raw);
        let g = c.groups.iter().find(|g| g.name == "pager").unwrap();
        assert_eq!(g.commands.get("bat"), Some(&Redirect::Tool(Tool::Read)));
    }

    #[test]
    fn merge_remove_and_disable() {
        let mut c = Config::builtin();
        let raw: RawConfig = toml::from_str(
            r#"
            [groups.read]
            remove = ["cat"]
            [groups.grep]
            disabled = true
        "#,
        )
        .unwrap();
        c.merge_raw(raw);
        let read = c.groups.iter().find(|g| g.name == "read").unwrap();
        assert!(!read.commands.contains_key("cat"));
        let grep = c.groups.iter().find(|g| g.name == "grep").unwrap();
        assert_eq!(grep.enabled, Some(false));
    }
}
