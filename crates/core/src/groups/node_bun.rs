//! `node-bun` pack - active when `bun` is on `PATH`. Nudges Node.js tooling
//! toward the (installed) bun equivalents.

use crate::config::Group;

pub(crate) fn def() -> Group {
    super::advice_group(
        "node-bun",
        super::req_cmd("bun"),
        &[
            ("node", "`bun run <file>`"),
            ("npm", "`bun install` / `bun run`"),
            ("npx", "`bunx`"),
        ],
    )
}
