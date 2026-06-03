//! Extract the program (and any subcommand words) a Bash command line invokes.
//!
//! Only the *leading* command is inspected - piped filters such as
//! `cmake --build . | grep error` are intentionally left alone, because the
//! dedicated tools cannot filter another command's stdout.
//!
//! We are deliberately *not* a full shell parser. The line is split into
//! quote-aware words, a few wrappers are peeled off the front
//! (subshell/group openers, `sudo`/`command`/`env`, `VAR=value` assignments),
//! and the program is reduced to its lowercased basename without `.exe`.

use std::collections::VecDeque;

/// Maximum subcommand words collected after the program, covering patterns like
/// `cargo build` and `git commit`.
const MAX_SUBCOMMAND_TOKENS: usize = 2;

/// Split a command line into quote-aware words: whitespace separates words,
/// but whitespace inside `"..."` or `'...'` is preserved and the quotes are
/// removed. This keeps a quoted program path with spaces
/// (`"C:\Program Files\rg.exe"`) as a single word.
fn shell_words(input: &str) -> VecDeque<String> {
    let mut words = VecDeque::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;

    for c in input.chars() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                current.push(c);
            }
        } else if c == '"' || c == '\'' {
            quote = Some(c);
            started = true;
        } else if c.is_whitespace() {
            if started {
                words.push_back(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(c);
            started = true;
        }
    }
    if started {
        words.push_back(current);
    }
    words
}

/// Is `word` a `VAR=value` environment assignment?
fn is_assignment(word: &str) -> bool {
    match word.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        None => false,
    }
}

/// Reduce a program word to its lowercased basename without a `.exe` suffix,
/// e.g. `C:\Program Files\RG.exe` -> `rg`. Returns `None` for the empty string
/// or the `.` shell builtin. Quotes are already removed by [`shell_words`], so
/// the word may legitimately contain spaces (a quoted path); we only stop at
/// shell operators, not at whitespace.
fn normalize_program(word: &str) -> Option<String> {
    let token = word.split([';', '|', '&', '<', '>', '(', ')']).next()?;
    let base = token.rsplit(['/', '\\']).next().unwrap_or(token).trim();
    let lowered = base.to_ascii_lowercase();
    let name = lowered.strip_suffix(".exe").unwrap_or(&lowered);
    (!name.is_empty() && name != ".").then(|| name.to_string())
}

/// Interpret `word` as a bare subcommand (`build`, `commit`, ...): it must start
/// with a letter; the leading `[A-Za-z0-9_-]` run is taken (lowercased).
fn subcommand(word: &str) -> Option<String> {
    if !word.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let token: String = word
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .collect();
    (!token.is_empty()).then(|| token.to_ascii_lowercase())
}

/// The normalized leading token sequence of a Bash command line:
/// `[program, subcommand?, subcommand?]`, e.g. `cargo build` ->
/// `["cargo", "build"]`. Empty if no program can be found.
pub fn command_tokens(command: &str) -> Vec<String> {
    let mut words = shell_words(command);

    // A leading subshell/group opener may be its own word ("(") or attached
    // ("(grep"). Strip the openers; drop the word if nothing else remains.
    if let Some(first) = words.front_mut() {
        let trimmed = first.trim_start_matches(['(', '{']).to_string();
        if trimmed.is_empty() {
            words.pop_front();
        } else {
            *first = trimmed;
        }
    }

    // Peel sudo/command/env wrappers and VAR=value assignments.
    while words
        .front()
        .is_some_and(|w| matches!(w.as_str(), "sudo" | "command" | "env") || is_assignment(w))
    {
        words.pop_front();
    }

    let Some(program) = words.pop_front().and_then(|w| normalize_program(&w)) else {
        return Vec::new();
    };

    // Up to two trailing subcommand words (stops at the first flag/path/operator).
    let mut tokens = vec![program];
    for word in words.iter().take(MAX_SUBCOMMAND_TOKENS) {
        match subcommand(word) {
            Some(sub) => tokens.push(sub),
            None => break,
        }
    }
    tokens
}

/// The leading program name only (basename, lowercased, `.exe` stripped), or
/// `None` if the command line has no recognizable leading program.
pub fn leading_program(command: &str) -> Option<String> {
    command_tokens(command).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prog(cmd: &str) -> Option<String> {
        leading_program(cmd)
    }

    #[test]
    fn plain_command() {
        assert_eq!(prog("grep -r foo ."), Some("grep".into()));
    }

    #[test]
    fn sudo_command_env_wrappers() {
        assert_eq!(prog("sudo grep foo"), Some("grep".into()));
        assert_eq!(prog("command env grep foo"), Some("grep".into()));
        // `sudoedit` must NOT be mistaken for a `sudo` wrapper.
        assert_eq!(prog("sudoedit x"), Some("sudoedit".into()));
    }

    #[test]
    fn env_assignments() {
        assert_eq!(prog("FOO=bar BAZ=1 grep x"), Some("grep".into()));
        assert_eq!(prog("RUST_LOG= cargo build"), Some("cargo".into()));
    }

    #[test]
    fn paths_and_exe_suffix() {
        assert_eq!(prog("/usr/local/bin/RG.exe -n x"), Some("rg".into()));
        assert_eq!(prog("bin\\rg x"), Some("rg".into()));
    }

    #[test]
    fn windows_drive_letter_path() {
        // The drive-letter colon must not truncate the program to "c".
        assert_eq!(prog("C:\\tools\\grep.exe -n x"), Some("grep".into()));
    }

    #[test]
    fn quoted_path_with_spaces_windows() {
        assert_eq!(
            prog(r#""C:\Program Files\rg.exe" -n pattern"#),
            Some("rg".into()),
        );
    }

    #[test]
    fn quoted_path_with_spaces_unix() {
        assert_eq!(
            prog(r#""/usr/local/Program Files/rg" -n pattern"#),
            Some("rg".into()),
        );
    }

    #[test]
    fn quote_and_group_openers() {
        assert_eq!(prog("\"grep\" x"), Some("grep".into()));
        assert_eq!(prog("( grep x )"), Some("grep".into()));
        assert_eq!(prog("(grep x)"), Some("grep".into()));
        assert_eq!(prog("{ grep x; }"), Some("grep".into()));
    }

    #[test]
    fn dot_builtin_is_not_a_program() {
        // `. foo` (source) is not a redirectable program.
        assert_eq!(prog(". foo"), None);
    }

    #[test]
    fn pipes_leave_following_filters_alone() {
        assert_eq!(prog("cmake --build . | grep error"), Some("cmake".into()));
    }

    #[test]
    fn empty_and_blank() {
        assert_eq!(prog("   "), None);
        assert_eq!(prog(""), None);
    }

    #[test]
    fn subcommand_tokens() {
        assert_eq!(
            command_tokens("cargo build --release"),
            vec!["cargo", "build"]
        );
        assert_eq!(command_tokens("git commit -m 'x'"), vec!["git", "commit"]);
        assert_eq!(command_tokens("cargo --version"), vec!["cargo"]);
        assert_eq!(
            command_tokens("python3 script.py"),
            vec!["python3", "script"]
        );
    }
}
