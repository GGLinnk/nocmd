//! `read` — always on. Redirect file-dumping commands to the Read tool.

use crate::config::Group;
use crate::redirect::Tool;

pub(crate) fn def() -> Group {
    super::tool_group("read", Tool::Read, &["cat", "head", "tail"])
}
