#![forbid(unsafe_code)]
//! `nocmd-core` - the command-gating engine behind the nocmd hook.
//!
//! Pipeline:
//!   1. [`parse::command_tokens`] turns a Bash command line into normalized
//!      leading tokens (`cargo build` -> `["cargo", "build"]`).
//!   2. [`config::Config`] holds the built-in groups plus any `.nocmd/*.toml`
//!      overrides; groups with a `requires` clause are conditional "packs".
//!   3. [`detect::Detect`] reports configured MCP servers and `PATH` commands.
//!   4. [`engine::Engine`] resolves which groups are active and matches a
//!      command line, yielding a [`engine::Decision`].

pub mod config;
pub mod detect;
pub mod engine;
pub mod groups;
pub mod parse;
pub mod redirect;

pub use config::{Config, Group, LoadError, RawConfig, Requires};
pub use detect::Detect;
pub use engine::{Decision, Engine};
pub use parse::{command_tokens, leading_program};
pub use redirect::{Redirect, Tool};
