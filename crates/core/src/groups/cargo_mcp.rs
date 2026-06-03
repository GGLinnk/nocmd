//! `cargo-mcp` pack — active when a configured MCP server name contains
//! `cargo`. Redirects each `cargo <sub>` to the matching MCP tool suffix; the
//! server name is filled in at render time (`mcp__cargo-mcp__cargo_build`).

use crate::config::Group;

pub(crate) fn def() -> Group {
    super::mcp_group(
        "cargo-mcp",
        "cargo",
        &[
            ("cargo build", "cargo_build"),
            ("cargo run", "cargo_run"),
            ("cargo test", "cargo_test"),
            ("cargo check", "cargo_check"),
            ("cargo clippy", "cargo_clippy"),
            ("cargo bench", "cargo_bench"),
            ("cargo add", "cargo_add"),
            ("cargo remove", "cargo_remove"),
            ("cargo update", "cargo_update"),
            ("cargo clean", "cargo_clean"),
            ("cargo fmt", "cargo_fmt_check"),
        ],
    )
}
