//! `git-mcp` pack - active when a configured MCP server name contains `git`.
//! Tool names vary between git MCP servers, so this redirects to the server
//! generically (`McpServer`) rather than guessing a specific tool.

use crate::config::Group;

pub(crate) fn def() -> Group {
    super::server_group(
        "git-mcp",
        "git",
        &[
            "git status",
            "git add",
            "git commit",
            "git log",
            "git diff",
            "git push",
            "git pull",
            "git checkout",
        ],
    )
}
