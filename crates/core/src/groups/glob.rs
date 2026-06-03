//! `glob` — always on. Redirect file-finding commands to the Glob tool.

use crate::config::Group;
use crate::redirect::Tool;

pub(crate) fn def() -> Group {
    super::tool_group("glob", Tool::Glob, &["find", "fd"])
}
