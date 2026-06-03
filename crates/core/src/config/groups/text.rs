//! `text` - always on. Text-processing commands with no single dedicated tool;
//! point at the closest combination instead.

use crate::config::Group;

pub(crate) fn def() -> Group {
    super::advice_group(
        "text",
        None,
        &[
            ("awk", "Read/Grep, or the Edit tool"),
            ("wc", "Read, or Grep output_mode \"count\""),
        ],
    )
}
