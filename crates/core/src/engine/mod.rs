//! The decision engine: resolve which groups are active against the detected
//! environment, then match a command line by longest token prefix.

use std::collections::HashMap;

use crate::config::{Config, Requires};
use crate::detect::Detect;
use crate::parse::command_tokens;

/// The outcome of evaluating a Bash command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// No active group matched - let the call proceed through the normal flow.
    Allow,
    /// A group matched: the call should be denied with this reason.
    Deny {
        reason: String,
        /// Name of the group that matched.
        group: String,
        /// The matched pattern (e.g. `cargo build`).
        matched: String,
    },
}

/// Holds a resolved configuration and the pattern index for fast matching.
pub struct Engine {
    config: Config,
    /// Normalized pattern -> index into `config.groups` (active groups only).
    index: HashMap<String, usize>,
    /// Per-group matched MCP server name, used to render MCP redirects.
    server_for: Vec<Option<String>>,
    /// Largest token count among indexed patterns (bounds the match loop).
    max_tokens: usize,
}

impl Engine {
    /// Resolve `config` against `detect` and build the matcher.
    pub fn new(config: Config, detect: &Detect) -> Self {
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut server_for = vec![None; config.groups.len()];
        let mut max_tokens = 1;

        for (i, group) in config.groups.iter().enumerate() {
            // Resolve the matched MCP server (drives both rendering and `requires`).
            server_for[i] = group
                .requires
                .as_ref()
                .and_then(|r| r.mcp.as_deref())
                .and_then(|needle| detect.matched_mcp(needle))
                .map(str::to_string);

            let enabled = match group.enabled {
                Some(b) => b,
                None => group
                    .requires
                    .as_ref()
                    .is_none_or(|r| requires_met(r, detect)),
            };
            if !enabled {
                continue;
            }
            for pattern in group.commands.keys() {
                max_tokens = max_tokens.max(pattern.split_whitespace().count());
                index.insert(pattern.clone(), i);
            }
        }

        Engine {
            config,
            index,
            server_for,
            max_tokens,
        }
    }

    /// Evaluate a Bash command line.
    pub fn evaluate(&self, command: &str) -> Decision {
        let tokens = command_tokens(command);
        if tokens.is_empty() {
            return Decision::Allow;
        }
        let upper = tokens.len().min(self.max_tokens);
        // Longest pattern first: `cargo build` beats a bare `cargo`.
        for n in (1..=upper).rev() {
            let pattern = tokens[..n].join(" ");
            let Some(&i) = self.index.get(&pattern) else {
                continue;
            };
            let group = &self.config.groups[i];
            let target = group.commands[&pattern].describe(self.server_for[i].as_deref());
            return Decision::Deny {
                reason: format!("Use {target} instead of \"{pattern}\"."),
                group: group.name.clone(),
                matched: pattern,
            };
        }
        Decision::Allow
    }

    /// Names of the groups that are currently active (sorted, deduped).
    pub fn active_groups(&self) -> Vec<&str> {
        let mut idxs: Vec<usize> = self.index.values().copied().collect();
        idxs.sort_unstable();
        idxs.dedup();
        idxs.iter()
            .map(|&i| self.config.groups[i].name.as_str())
            .collect()
    }
}

fn requires_met(r: &Requires, detect: &Detect) -> bool {
    let checks = [
        r.mcp.as_deref().map(|m| detect.has_mcp(m)),
        r.command.as_deref().map(|c| detect.has_command(c)),
    ];
    let mut checks = checks.into_iter().flatten().peekable();
    if checks.peek().is_none() {
        return true;
    }
    if r.any {
        checks.any(|ok| ok)
    } else {
        checks.all(|ok| ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn engine_no_detect() -> Engine {
        Engine::new(Config::builtin(), &Detect::empty())
    }

    fn forced(toml_src: &str) -> Engine {
        let mut cfg = Config::builtin();
        cfg.merge_raw(toml::from_str(toml_src).unwrap());
        Engine::new(cfg, &Detect::empty())
    }

    #[test]
    fn redirects_grep_to_grep_group() {
        match engine_no_detect().evaluate("grep -r foo .") {
            Decision::Deny {
                group,
                matched,
                reason,
            } => {
                assert_eq!(group, "grep");
                assert_eq!(matched, "grep");
                assert!(reason.contains("the Grep tool"));
            }
            other => panic!("expected deny, got {other:?}"),
        }
    }

    #[test]
    fn redirects_sed_to_edit_group() {
        match engine_no_detect().evaluate("sed -i s/a/b/ f") {
            Decision::Deny { group, reason, .. } => {
                assert_eq!(group, "edit");
                assert!(reason.contains("the Edit tool"));
            }
            other => panic!("expected deny, got {other:?}"),
        }
    }

    #[test]
    fn allows_piped_grep_and_unknown() {
        let e = engine_no_detect();
        assert_eq!(e.evaluate("cmake --build . | grep error"), Decision::Allow);
        assert_eq!(e.evaluate("ls -la"), Decision::Allow);
    }

    #[test]
    fn packs_off_without_detection() {
        let e = engine_no_detect();
        assert_eq!(e.evaluate("cargo build --release"), Decision::Allow);
        assert_eq!(e.evaluate("node server.js"), Decision::Allow);
    }

    #[test]
    fn cargo_pack_when_forced() {
        let e = forced("[groups.cargo-mcp]\nenabled = true\n");
        match e.evaluate("cargo build --release") {
            Decision::Deny {
                group,
                matched,
                reason,
            } => {
                assert_eq!(group, "cargo-mcp");
                assert_eq!(matched, "cargo build");
                // No detection -> falls back to naming the suffix.
                assert!(reason.contains("cargo_build"));
            }
            other => panic!("expected deny, got {other:?}"),
        }
    }

    #[test]
    fn longest_prefix_wins() {
        let e = forced(
            "[groups.custom]\nenabled = true\n[groups.custom.advice]\n\"cargo\" = \"bare\"\n\"cargo build\" = \"two\"\n",
        );
        match e.evaluate("cargo build") {
            Decision::Deny { matched, .. } => assert_eq!(matched, "cargo build"),
            other => panic!("expected deny, got {other:?}"),
        }
        match e.evaluate("cargo run") {
            Decision::Deny { matched, .. } => assert_eq!(matched, "cargo"),
            other => panic!("expected deny, got {other:?}"),
        }
    }
}
