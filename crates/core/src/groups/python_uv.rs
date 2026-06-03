//! `python-uv` pack — active when `uv` is on `PATH`. Nudges Python invocations
//! toward `uv` for reproducible, isolated runs.

use crate::config::Group;

pub(crate) fn def() -> Group {
    super::advice_group(
        "python-uv",
        super::req_cmd("uv"),
        &[
            ("python", "`uv run python …`"),
            ("python3", "`uv run python …`"),
            ("pip", "`uv pip …`"),
            ("pip3", "`uv pip …`"),
        ],
    )
}
