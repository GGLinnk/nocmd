//! What a blocked command should be done with instead.
//!
//! The deny message is derived from these types (see [`Redirect::describe`]):
//! adding a redirect target is a new enum variant the compiler forces every
//! match arm to handle.

use serde::Deserialize;

/// A built-in Claude Code tool that supersedes a shell command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Grep,
    Glob,
    Read,
    Edit,
}

impl Tool {
    /// How the tool is referred to in a deny message.
    pub fn description(self) -> &'static str {
        match self {
            Tool::Grep => "the Grep tool",
            Tool::Glob => "the Glob tool",
            Tool::Read => "the Read tool",
            Tool::Edit => "the Edit tool",
        }
    }
}

/// Where a matched command is redirected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Redirect {
    /// A built-in Claude tool.
    Tool(Tool),
    /// A specific MCP tool. The stored value is the tool's suffix; the server
    /// name is supplied at render time, producing `mcp__<server>__<suffix>`.
    Mcp(String),
    /// The MCP server generically (used when the exact tool name varies).
    McpServer,
    /// Free-form guidance (no single canonical tool).
    Advice(String),
}

impl Redirect {
    /// Render the "use ___" phrase. `server` is the MCP server name resolved
    /// from detection, if any (only consulted by the MCP variants).
    pub fn describe(&self, server: Option<&str>) -> String {
        match self {
            Redirect::Tool(tool) => tool.description().to_string(),
            Redirect::Mcp(suffix) => match server {
                Some(s) => format!("mcp__{s}__{suffix}"),
                None => format!("the MCP tool `{suffix}`"),
            },
            Redirect::McpServer => match server {
                Some(s) => format!("the {s} MCP"),
                None => "the configured MCP server".to_string(),
            },
            Redirect::Advice(advice) => advice.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_describes_itself() {
        assert_eq!(Redirect::Tool(Tool::Grep).describe(None), "the Grep tool");
    }

    #[test]
    fn mcp_expands_with_server() {
        let r = Redirect::Mcp("cargo_build".to_string());
        assert_eq!(r.describe(Some("cargo-mcp")), "mcp__cargo-mcp__cargo_build");
        assert_eq!(r.describe(None), "the MCP tool `cargo_build`");
    }

    #[test]
    fn server_variant() {
        assert_eq!(
            Redirect::McpServer.describe(Some("git-mcp")),
            "the git-mcp MCP"
        );
    }
}
