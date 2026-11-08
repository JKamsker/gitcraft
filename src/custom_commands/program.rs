//! What a custom command runs: its variables set, each one quoted for `sh` as one word
//! (whatever is in it: spaces, quotes, `;`, `$(…)`, line breaks), then its command line, which
//! reads them as `$SHA`, `$BRANCH`… The values are never put into the command line's text.

use crate::remote::sh_quote;

/// What a command is run about, from the menu it was picked in. Empty when it doesn't apply
/// (no file outside a file's menu).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    /// The commit's full hash.
    pub sha: String,
    pub branch: String,
    pub remote: String,
    /// Relative to the repository, `/` between names.
    pub file: String,
}

/// Short hashes, as the rest of gitcraft shows them.
const SHORT: usize = 8;

/// Every variable the command gets: gitcraft's own, then the prompts' answers. `repo` is the
/// repository's folder as its machine names it.
pub fn variables(scope: &Scope, repo: &str, answers: &[(String, String)]) -> Vec<(String, String)> {
    let short: String = scope.sha.chars().take(SHORT).collect();
    let mut vars = vec![
        ("SHA".to_owned(), scope.sha.clone()),
        ("SHORT_SHA".to_owned(), short),
        ("BRANCH".to_owned(), scope.branch.clone()),
        ("REMOTE".to_owned(), scope.remote.clone()),
        ("FILE".to_owned(), scope.file.clone()),
        ("REPO".to_owned(), repo.to_owned()),
    ];
    vars.extend(answers.iter().cloned());
    vars
}

/// The script for a command whose output is collected: the variables, exported, then the
/// command line in a subshell (an `exit` in it ends only the command). It exits as the command
/// does.
pub fn captured_program(vars: &[(String, String)], command: &str) -> String {
    // No arguments: what the command reads as `$1`, `$@` is nothing, wherever it runs.
    let mut script = String::from("set --\n");
    for (name, value) in vars {
        // Names were checked (see `variable_error`); values can be anything.
        script.push_str(&format!("{name}={}\nexport {name}\n", sh_quote(value)));
    }
    script.push_str(&format!("GITCRAFT_COMMAND={}\n", sh_quote(command)));
    script.push_str("( eval \"$GITCRAFT_COMMAND\" )\n");
    script
}

/// The script for a command run in a terminal of its own: [`captured_program`], then what it
/// ended with, and the terminal stays until Enter is pressed so its output can be read.
pub fn terminal_program(vars: &[(String, String)], command: &str, name: &str) -> String {
    let mut script = captured_program(vars, command);
    script.push_str("gitcraft_status=$?\n");
    script.push_str(&format!(
        "printf '\\n%s: exit status %s. Press Enter to close.\\n' {} \"$gitcraft_status\"\n",
        sh_quote(name)
    ));
    script.push_str("read -r gitcraft_line\nexit \"$gitcraft_status\"\n");
    script
}

/// Lines of output a toast shows, at most (the last ones).
const TOAST_LINES: usize = 8;
/// Characters of output a toast shows, at most (the last ones).
const TOAST_CHARS: usize = 600;

/// The toast for a command that ended with `code`: (text, failed). The end of what it printed
/// (both outputs), without colours or progress lines rewritten in place.
pub fn toast_text(name: &str, code: i32, stdout: &[u8], stderr: &[u8]) -> (String, bool) {
    let failed = code != 0;
    let out = clean(&String::from_utf8_lossy(stdout));
    let err = clean(&String::from_utf8_lossy(stderr));
    let printed = match (out.is_empty(), err.is_empty()) {
        (_, true) => out,
        (true, false) => err,
        // stderr last: a failure's reason, which the end of the toast keeps.
        (false, false) => format!("{out}\n{err}"),
    };
    let head = if failed {
        format!("{name} failed (exit status {code})")
    } else {
        format!("{name} finished")
    };
    let tail = tail(&printed);
    let text = if tail.is_empty() {
        head
    } else {
        format!("{head}\n{tail}")
    };
    (text, failed)
}

/// Opt-in presentation from the actual outcome. The raw toast producer stays stable.
/// This resolves the emission locale; retain these input fields and call at paint when
/// a longer-lived result must follow subsequent language changes.
pub fn display_toast_text(name: &str, code: i32, stdout: &[u8], stderr: &[u8]) -> (String, bool) {
    let (raw, failed) = toast_text(name, code, stdout, stderr);
    let raw_head = if failed {
        format!("{name} failed (exit status {code})")
    } else {
        format!("{name} finished")
    };
    let head = if failed {
        crate::i18n::message(
            "interaction-custom-display-failed",
            &[("name", name.into()), ("code", code.into())],
        )
    } else {
        crate::i18n::message(
            "interaction-custom-display-finished",
            &[("name", name.into())],
        )
    };
    // The prefix is constructed from the same typed outcome; output is never reparsed.
    (
        head + raw.strip_prefix(&raw_head).unwrap_or_default(),
        failed,
    )
}

/// The last [`TOAST_LINES`] lines of `text`, no more than [`TOAST_CHARS`] characters.
fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let from = lines.len().saturating_sub(TOAST_LINES);
    let kept = lines.get(from..).unwrap_or_default().join("\n");
    let count = kept.chars().count();
    if count <= TOAST_CHARS {
        return kept;
    }
    let rest: String = kept.chars().skip(count - TOAST_CHARS).collect();
    format!("…{rest}")
}

/// `text` without terminal colour codes, each line as it was left (after its last `\r`),
/// trimmed.
fn clean(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // `ESC [ … letter`, or the one character after a lone ESC.
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() || n == '~' {
                        break;
                    }
                }
            } else {
                chars.next();
            }
            continue;
        }
        plain.push(c);
    }
    let lines: Vec<&str> = plain
        .lines()
        .map(|line| {
            line.rsplit('\r')
                .find(|part| !part.trim().is_empty())
                .unwrap_or_default()
                .trim_end()
        })
        .collect();
    lines.join("\n").trim().to_owned()
}

#[cfg(test)]
mod display_tests {
    use super::*;
    #[test]
    fn typed_toast_display_keeps_failed_flag_and_cleaned_external_tail_unchanged() {
        let name = "Name\nfailed (exit status 5): {$x} 色";
        for code in [0, 5] {
            let raw = toast_text(
                name,
                code,
                b"opaque stdout\n",
                b"\x1b[31mprovider error: finished\x1b[0m\n",
            );
            crate::i18n::set_language("de");
            let shown = display_toast_text(
                name,
                code,
                b"opaque stdout\n",
                b"\x1b[31mprovider error: finished\x1b[0m\n",
            );
            assert_eq!(shown.1, raw.1);
            assert!(shown.0.starts_with(name));
            assert!(
                shown
                    .0
                    .ends_with("\nopaque stdout\nprovider error: finished")
            );
            assert!(shown.0.contains(if code == 0 {
                "abgeschlossen"
            } else {
                "fehlgeschlagen (Exitstatus 5)"
            }));
            crate::i18n::set_language("en-US");
            assert_eq!(
                display_toast_text(
                    name,
                    code,
                    b"opaque stdout\n",
                    b"\x1b[31mprovider error: finished\x1b[0m\n"
                ),
                raw
            );
        }
    }
}
