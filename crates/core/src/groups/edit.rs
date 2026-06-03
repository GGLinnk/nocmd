//! `edit` — always on. Redirect in-place stream editing to the Edit tool.

use crate::config::Group;
use crate::redirect::Tool;

pub(crate) fn def() -> Group {
    super::tool_group("edit", Tool::Edit, &["sed"])
}
