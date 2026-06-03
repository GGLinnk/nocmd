//! Built-in groups, one module per group.
//!
//! Each submodule exposes a `def() -> Group`. [`builtins`] assembles them in
//! precedence order (later groups win ties when two patterns collide). Add a
//! new built-in by creating a file here and listing its `def()` in [`builtins`].
//!
//! The base groups (`grep`/`glob`/`read`/`edit`/`text`) are always on; the rest
//! are conditional "packs" gated on a [`crate::config::Requires`] clause.

use std::collections::BTreeMap;

use crate::config::{normalize_pattern, Group, Requires};
use crate::redirect::{Redirect, Tool};

mod cargo_mcp;
mod edit;
mod git_mcp;
mod glob;
mod grep;
mod node_bun;
mod python_uv;
mod read;
mod text;

/// All built-in groups, in precedence order.
pub fn builtins() -> Vec<Group> {
    vec![
        // Always-on base groups, one per target tool.
        grep::def(),
        glob::def(),
        read::def(),
        edit::def(),
        text::def(),
        // Conditional packs (active only when detected).
        cargo_mcp::def(),
        git_mcp::def(),
        node_bun::def(),
        python_uv::def(),
    ]
}

fn group(name: &str, requires: Option<Requires>, commands: BTreeMap<String, Redirect>) -> Group {
    Group {
        name: name.to_string(),
        enabled: None,
        requires,
        commands,
    }
}

fn map<'a, V>(pairs: impl IntoIterator<Item = (&'a str, V)>) -> BTreeMap<String, Redirect>
where
    V: Into<Redirect>,
{
    pairs
        .into_iter()
        .map(|(pat, v)| (normalize_pattern(pat), v.into()))
        .collect()
}

/// A group that redirects every command to one built-in [`Tool`].
pub(crate) fn tool_group(name: &str, tool: Tool, commands: &[&str]) -> Group {
    group(name, None, map(commands.iter().map(|&c| (c, tool))))
}

/// A group of free-form advice redirects, with an optional `requires` clause.
pub(crate) fn advice_group(
    name: &str,
    requires: Option<Requires>,
    pairs: &[(&str, &str)],
) -> Group {
    let commands = pairs
        .iter()
        .map(|&(pat, advice)| (normalize_pattern(pat), Redirect::Advice(advice.to_string())))
        .collect();
    group(name, requires, commands)
}

/// A pack mapping `pattern -> MCP tool suffix`, gated on an MCP server.
pub(crate) fn mcp_group(name: &str, server: &str, pairs: &[(&str, &str)]) -> Group {
    let commands = pairs
        .iter()
        .map(|&(pat, suffix)| (normalize_pattern(pat), Redirect::Mcp(suffix.to_string())))
        .collect();
    group(name, req_mcp(server), commands)
}

/// A pack that redirects commands to an MCP server generically, gated on it.
pub(crate) fn server_group(name: &str, server: &str, commands: &[&str]) -> Group {
    let commands = commands
        .iter()
        .map(|&c| (normalize_pattern(c), Redirect::McpServer))
        .collect();
    group(name, req_mcp(server), commands)
}

fn req_mcp(server: &str) -> Option<Requires> {
    Some(Requires {
        mcp: Some(server.to_string()),
        command: None,
        any: false,
    })
}

/// A `requires` clause satisfied when command `cmd` resolves on `PATH`.
pub(crate) fn req_cmd(cmd: &str) -> Option<Requires> {
    Some(Requires {
        mcp: None,
        command: Some(cmd.to_string()),
        any: false,
    })
}

impl From<Tool> for Redirect {
    fn from(tool: Tool) -> Self {
        Redirect::Tool(tool)
    }
}
