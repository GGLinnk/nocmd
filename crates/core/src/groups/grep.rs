//! `grep` - always on. Redirect text-search commands to the Grep tool.

use crate::config::Group;
use crate::redirect::Tool;

pub(crate) fn def() -> Group {
    super::tool_group(
        "grep",
        Tool::Grep,
        &[
            "grep",
            "egrep",
            "fgrep",
            "rg",
            "ripgrep",
            "ag",
            "ack",
            "findstr",
            "select-string",
        ],
    )
}
