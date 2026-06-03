#![forbid(unsafe_code)]
//! `nocmd` — a Claude Code PreToolUse hook for the Bash tool.
//!
//! With no subcommand this runs as the hook: it reads the PreToolUse event as
//! JSON on stdin and, if the leading command matches an active group, emits a
//! `deny` decision as JSON on stdout. Anything unexpected fails OPEN (exit 0,
//! no decision) so a broken hook can never brick the Bash tool.
//!
//! The `check` and `detect` subcommands are for humans inspecting behavior.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use nocmd_core::{Config, Decision, Detect, Engine};
use serde::Deserialize;

/// A Claude Code PreToolUse hook for the Bash tool that redirects/blocks
/// discouraged commands; configurable via .nocmd TOML groups.
#[derive(Parser)]
#[command(name = "nocmd", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show how a Bash command line would be handled (ALLOW or DENY + reason).
    Check {
        /// The command line to evaluate, e.g. `nocmd check cargo build --release`.
        #[arg(required = true, allow_hyphen_values = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    /// Show detected MCP servers / PATH tools and which groups are active here.
    Detect,
}

#[derive(Deserialize, Default)]
struct ToolInput {
    #[serde(default)]
    command: String,
}

#[derive(Deserialize)]
struct HookEvent {
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_input: ToolInput,
    #[serde(default)]
    cwd: Option<String>,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        None => {
            // Default: run as the hook. Always exit 0 (fail open).
            run_hook();
            ExitCode::SUCCESS
        }
        Some(Command::Check { command }) => cmd_check(&command.join(" ")),
        Some(Command::Detect) => cmd_detect(),
    }
}

/// Engine for the hook path: `.nocmd` load errors are swallowed (fail open).
fn engine_for(cwd: &Path) -> Engine {
    Engine::new(Config::discover(cwd), &Detect::new(cwd))
}

/// Engine for human-facing subcommands: `.nocmd` load errors are reported to
/// stderr so a typo'd TOML file doesn't silently misbehave.
fn engine_for_human(cwd: &Path) -> Engine {
    let (cfg, errs) = Config::discover_verbose(cwd);
    for e in &errs {
        eprintln!("nocmd: warning: {e}");
    }
    Engine::new(cfg, &Detect::new(cwd))
}

fn cwd_or_dot() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Run as the PreToolUse hook. Any failure is swallowed (fail open).
fn run_hook() {
    let _ = try_hook();
}

fn try_hook() -> Option<()> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).ok()?;
    if raw.trim().is_empty() {
        return Some(());
    }
    let event: HookEvent = serde_json::from_str(&raw).ok()?;
    if event.tool_name != "Bash" {
        return Some(());
    }
    if event.tool_input.command.trim().is_empty() {
        return Some(());
    }
    let cwd = event
        .cwd
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    if let Decision::Deny { reason, .. } = engine_for(&cwd).evaluate(&event.tool_input.command) {
        emit_deny(&reason);
    }
    Some(())
}

fn emit_deny(reason: &str) {
    let payload = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason
        }
    });
    if let Ok(s) = serde_json::to_string(&payload) {
        println!("{s}");
    }
}

fn cmd_check(command: &str) -> ExitCode {
    if command.trim().is_empty() {
        eprintln!("usage: nocmd check \"<bash command line>\"");
        return ExitCode::from(2);
    }
    match engine_for_human(&cwd_or_dot()).evaluate(command) {
        Decision::Allow => {
            println!("ALLOW");
            ExitCode::SUCCESS
        }
        Decision::Deny {
            reason,
            group,
            matched,
        } => {
            println!("DENY  [{group}] matched \"{matched}\"");
            println!("{reason}");
            ExitCode::from(2)
        }
    }
}

fn cmd_detect() -> ExitCode {
    let cwd = cwd_or_dot();
    let detect = Detect::new(&cwd);
    let (cfg, errs) = Config::discover_verbose(&cwd);
    for e in &errs {
        eprintln!("nocmd: warning: {e}");
    }
    let engine = Engine::new(cfg, &detect);

    println!("cwd: {}", cwd.display());

    let servers: Vec<&str> = detect.mcp_servers().collect();
    if servers.is_empty() {
        println!("MCP servers : (none configured)");
    } else {
        println!("MCP servers : {}", servers.join(", "));
    }
    println!("bun on PATH : {}", yesno(detect.has_command("bun")));
    println!("uv on PATH  : {}", yesno(detect.has_command("uv")));

    let groups = engine.active_groups();
    println!("active group: {}", groups.join(", "));
    ExitCode::SUCCESS
}

fn yesno(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}
