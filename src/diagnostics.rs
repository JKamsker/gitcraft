//! Help → Copy diagnostics: what a bug report needs to know about this install, as Markdown.
//! gitcraft's version and channel, the system, the graphics adapter, the versions of git, `gh`,
//! `glab` and `tea` and whether each is signed in, and the git settings of the doctor's
//! allowlist with the scope and file name they come from. Never a token, an account, a name
//! or an email; a setting whose value is a path says only that it is set, and any folder of a
//! user's (`/home/x`, `C:\Users\x`, `/mnt/c/Users/x`…) is written `~`.

pub(crate) mod crash;

use std::path::{Path, PathBuf};

use crate::git::doctor::config::{self, SETTINGS};

/// What the window knows that the report needs.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// The update channel, as Settings shows it.
    pub channel: String,
    /// The graphics adapter and backend wgpu draws with.
    pub adapter: Option<String>,
    /// The repository of the tab shown, if any: its settings are reported.
    pub repo: Option<PathBuf>,
}

/// The settings left out of the report even though the doctor shows them: who someone is
/// (name, email, signing key).
const PERSONAL: &[&str] = &["user.name", "user.email", "user.signingKey"];

/// Collects the report. Runs git and the forges' tools: not on the window's thread.
pub fn collect(facts: &Facts) -> String {
    // Each tool may take a few seconds (glab asks its server): side by side.
    let tools: Vec<crate::forge::ToolReport> = std::thread::scope(|scope| {
        let asked: Vec<_> = ["gh", "glab", "tea"]
            .into_iter()
            .map(|tool| {
                scope.spawn(crate::connections::inherit(move || {
                    crate::forge::tool_report(tool)
                }))
            })
            .collect();
        asked.into_iter().filter_map(|t| t.join().ok()).collect()
    });
    let git = crate::git::version().unwrap_or_else(|e| format!("not found ({e})"));
    let settings = facts.repo.as_deref().map(repo_settings);
    report(facts, &git, &tools, settings.as_ref(), &home())
}

/// The report from what was collected; `home` is written `~`.
pub fn report(
    facts: &Facts,
    git: &str,
    tools: &[crate::forge::ToolReport],
    settings: Option<&Result<Vec<String>, String>>,
    home: &str,
) -> String {
    let mut out = String::from("### gitcraft diagnostics\n\n");
    let mut line = |label: &str, value: &str| out.push_str(&format!("- **{label}:** {value}\n"));
    line(
        "gitcraft",
        &format!("{} ({} channel)", crate::update::CURRENT, facts.channel),
    );
    line("System", &system());
    line(
        "Graphics",
        facts
            .adapter
            .as_deref()
            .unwrap_or("not known (no wgpu adapter)"),
    );
    line("git", git);
    if let Some((active, queued)) = crate::tasks::counts() {
        line(
            "Background jobs",
            &format!(
                "{active}/{} active, {queued}/{} queued (peak {})",
                crate::tasks::WORKERS,
                crate::tasks::QUEUE_CAPACITY,
                crate::tasks::peak_queued().unwrap_or(0)
            ),
        );
    }
    if let Some(counts) = crate::processes::counts() {
        line(
            "Managed processes",
            &format!(
                "{} active, {} queued, {} started, peak {}",
                counts.active, counts.queued, counts.spawned, counts.peak
            ),
        );
    }
    let operations = crate::git::backend::metrics();
    let native: u64 = operations.iter().map(|m| m.native_calls).sum();
    let fallback: u64 = operations.iter().map(|m| m.fallback_calls).sum();
    let cli: u64 = operations.iter().map(|m| m.subprocesses).sum();
    line(
        "Git read backend",
        &format!("{native} native reads, {fallback} read fallbacks, {cli} CLI invocations"),
    );
    for operation in operations.iter().filter(|m| m.fallback_calls > 0) {
        let reasons = operation
            .fallback_reasons
            .iter()
            .map(|(reason, count)| format!("{reason}: {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        line(&format!("Git {} fallback", operation.operation), &reasons);
    }
    for t in tools {
        // glab can only tell by asking its server: offline reads as a failed check.
        let (yes, no) = if t.tool == "glab" {
            ("auth status ok", "auth status failed")
        } else {
            ("signed in", "not signed in")
        };
        let state = match (&t.version, t.signed_in) {
            (None, _) => "not found (or didn't answer)".to_owned(),
            (Some(v), Some(true)) => format!("{v}, {yes}"),
            (Some(v), Some(false)) => format!("{v}, {no}"),
            (Some(v), None) => format!("{v}, sign-in not known"),
        };
        line(t.tool, &state);
    }
    match settings {
        None => out.push_str("\nNo repository open: no git settings.\n"),
        // The error names paths and machines: left out.
        Some(Err(_)) => out.push_str("\nGit settings: could not be read.\n"),
        Some(Ok(rows)) if rows.is_empty() => {
            out.push_str("\nGit settings: none of the reported ones is set.\n")
        }
        Some(Ok(rows)) => {
            out.push_str("\nGit settings in effect (value · scope · file name):\n\n```\n");
            for row in rows {
                out.push_str(row);
                out.push('\n');
            }
            out.push_str("```\n");
        }
    }
    if let Some(panic) = crash::last() {
        out.push_str("\nLast panic (local log; paths and known secret forms redacted):\n\n");
        out.push_str(&panic);
        out.push('\n');
    }
    out.push_str("\nRecent operations (arguments, output and paths omitted):\n");
    for row in crash::recent() {
        out.push_str("- ");
        out.push_str(&row);
        out.push('\n');
    }
    redact_users(&redact_home(&out, home))
}

/// The settings of the doctor's allowlist in effect in `repo`, but the personal ones: `key =
/// value · scope · file`, where the repository is (here, WSL, SSH) said without its path.
fn repo_settings(repo: &Path) -> Result<Vec<String>, String> {
    let all = config::origins(repo)?;
    let mut rows = vec![format!(
        "# repository {}",
        if crate::remote::is_remote(repo) {
            "on another machine (WSL or SSH)"
        } else {
            "on this machine"
        }
    )];
    rows.extend(settings_rows(&all));
    Ok(rows)
}

/// One line per reported setting that is set, the value in effect.
pub fn settings_rows(all: &[config::Origin]) -> Vec<String> {
    SETTINGS
        .iter()
        .filter(|s| !PERSONAL.iter().any(|p| p.eq_ignore_ascii_case(s.key)))
        .filter_map(|s| {
            let winner = config::values_of(all, s.key).into_iter().last()?;
            let value = winner.shown_value();
            // A path (an editor, a program, a folder) can name its user: only that it is set.
            let value = if value.contains(['/', '\\']) {
                "(a path)"
            } else {
                value
            };
            let file = winner
                .file
                .as_deref()
                .map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f))
                .unwrap_or("-");
            Some(format!(
                "{} = {value} · {} · {file}",
                s.key,
                winner.scope.label(),
            ))
        })
        .collect()
}

/// The system: its name and version where it says them, and the processor.
fn system() -> String {
    let arch = std::env::consts::ARCH;
    let name = os_name().unwrap_or_else(|| std::env::consts::OS.to_owned());
    format!("{name} ({arch})")
}

#[cfg(target_os = "linux")]
fn os_name() -> Option<String> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    pretty_name(&text)
}

#[cfg(not(target_os = "linux"))]
fn os_name() -> Option<String> {
    None
}

/// `PRETTY_NAME` of an os-release file, unquoted.
pub fn pretty_name(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim().trim_matches('"').to_owned())
        .filter(|v| !v.is_empty())
}

/// This user's home folder, as the report writes it `~`.
fn home() -> String {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `text` with the home folder (as written with `\` or `/`, in any case) replaced by `~`, where
/// it is a whole path (followed by a separator, a space or the end).
pub fn redact_home(text: &str, home: &str) -> String {
    let home = home.trim_end_matches(['/', '\\']);
    if home.len() < 2 {
        return text.to_owned();
    }
    let mut out = text.to_owned();
    for form in [home.to_owned(), home.replace('\\', "/")] {
        out = replace_whole(&out, &form, "~");
    }
    out
}

/// `text` with every `needle` (ASCII case ignored) that ends where a path component does
/// replaced by `with`.
fn replace_whole(text: &str, needle: &str, with: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let wanted = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(&wanted) {
        let start = at + found;
        let end = start + wanted.len();
        let whole = text[end..]
            .chars()
            .next()
            .is_none_or(|c| matches!(c, '/' | '\\' | '`') || c.is_whitespace());
        out.push_str(&text[at..start]);
        out.push_str(if whole { with } else { &text[start..end] });
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// `text` with the folder named after any user written `~`: what follows `/home/`, `/Users/`
/// or `\Users\` (also `/mnt/c/Users/`, `/c/Users/`, `C:/Users/`), and `/root`, on whichever
/// machine (WSL, SSH, Windows) the path is.
pub fn redact_users(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let next = ["/home/", "/users/", "\\users\\"]
            .iter()
            .filter_map(|m| lower[at..].find(m).map(|i| (at + i, m.len())))
            .min();
        let Some((start, len)) = next else {
            break;
        };
        // Back to the start of the path: a drive (`C:`), `/mnt/c`, `/c`.
        let path_start = text[at..start]
            .rfind(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '\'' | '(' | '='))
            .map_or(at, |i| at + i + 1);
        let name_end = text[start + len..]
            .find(|c: char| matches!(c, '/' | '\\' | '`') || c.is_whitespace())
            .map_or(text.len(), |i| start + len + i);
        out.push_str(&text[at..path_start]);
        out.push('~');
        at = name_end;
    }
    out.push_str(&text[at..]);
    replace_whole(&out, "/root", "~")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::ToolReport;

    #[test]
    fn the_report_says_what_it_should_and_nothing_personal() {
        let facts = Facts {
            channel: "dev".into(),
            adapter: Some("llvmpipe (Vulkan)".into()),
            repo: None,
        };
        let all = config::parse_origins(
            "global\0file:/home/zoë/.gitconfig\0user.email\nzoe@example.com\0\
             global\0file:/home/zoë/.gitconfig\0user.name\nZoë\0\
             global\0file:/home/zoë/.gitconfig\0pull.rebase\ntrue\0\
             local\0file:.git/config\0pull.rebase\nfalse\0\
             local\0file:.git/config\0credential.helper\nstore\0\
             global\0file:/home/zoë/.gitconfig\0core.editor\n/home/zoë/bin/ed\0",
        );
        let rows = settings_rows(&all);
        assert_eq!(
            rows,
            [
                "pull.rebase = false · Repository · config",
                "core.editor = (a path) · Global · .gitconfig"
            ]
        );
        let tools = [
            ToolReport {
                tool: "gh",
                version: Some("gh version 2.89.0".into()),
                signed_in: Some(true),
            },
            ToolReport {
                tool: "glab",
                version: Some("glab 1.50".into()),
                signed_in: Some(false),
            },
            ToolReport {
                tool: "tea",
                version: None,
                signed_in: None,
            },
        ];
        let settings = Ok(vec![
            "pull.rebase = true · Global · .gitconfig".to_owned(),
            r"# a WSL path /mnt/c/Users/Ana/x and C:\Users\Ana\y".to_owned(),
        ]);
        let text = report(
            &facts,
            "git version 2.43.0",
            &tools,
            Some(&settings),
            "/home/zoë",
        );
        assert!(text.contains("dev channel"));
        assert!(text.contains("llvmpipe (Vulkan)"));
        assert!(text.contains("gh version 2.89.0, signed in"));
        assert!(text.contains("glab 1.50, auth status failed"));
        assert!(text.contains("**tea:** not found"));
        assert!(text.contains("pull.rebase = true · Global · .gitconfig"));
        assert!(text.contains(r"a WSL path ~/x and ~\y"), "{text}");
        assert!(!text.contains("zoë") && !text.contains("example.com") && !text.contains("Ana"));
        let none = report(&facts, "git", &[], None, "");
        assert!(none.contains("No repository open"));
        let failed = report(
            &facts,
            "git",
            &[],
            Some(&Err("ssh://bob@x/home/bob".into())),
            "",
        );
        assert!(failed.contains("could not be read.") && !failed.contains("bob"));
    }

    #[test]
    fn home_and_users_are_redacted() {
        assert_eq!(
            redact_home(r"C:\Users\ana\x and c:/users/ana/y", r"C:\Users\ana"),
            "~\\x and ~/y"
        );
        // Only a whole folder: /home/al isn't the start of /home/alice.
        assert_eq!(
            redact_home("/home/alice/x /rootless", "/home/al"),
            "/home/alice/x /rootless"
        );
        assert_eq!(redact_home("/home/al/x", "/home/al"), "~/x");
        // Too short to be a home folder: nothing replaced.
        assert_eq!(redact_home("/a/b", "/"), "/a/b");
        // Any user's folder, here or on another machine, in every form.
        assert_eq!(
            redact_users(
                "/home/bob/.gitconfig /mnt/c/Users/Ana/gpg.exe /c/Users/ana C:/Users/x/y /root/z"
            ),
            "~/.gitconfig ~/gpg.exe ~ ~/y ~/z"
        );
        assert_eq!(redact_users("no paths /rootless"), "no paths /rootless");
        assert_eq!(
            pretty_name("NAME=x\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\n"),
            Some("Ubuntu 24.04 LTS".into())
        );
        assert_eq!(pretty_name(""), None);
    }

    /// Against the real git and tools of this machine: collected without a panic, and the
    /// repository's settings read.
    #[test]
    fn collecting_works_here() -> Result<(), String> {
        let dir = crate::git::doctor::tests::repo("diagnostics", &[])?;
        crate::git::doctor::tests::git(&dir, &["config", "pull.ff", "only"])?;
        let facts = Facts {
            channel: "main".into(),
            adapter: None,
            repo: Some(dir.clone()),
        };
        let text = collect(&facts);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(text.contains("**git:** git version"), "{text}");
        assert!(
            text.contains("pull.ff = only · Repository · config"),
            "{text}"
        );
        assert!(text.contains("repository on this machine"));
        Ok(())
    }
}

/// A bounded prefilled issue URL. Full diagnostics remain on the clipboard.
pub(crate) fn issue_url(report: &str) -> String {
    let mut body = String::from("Please describe what happened and how to reproduce it.\n\n");
    for line in report.lines() {
        if encode(&(body.clone() + line)).len() > 6500 {
            body.push_str("\n[Remaining diagnostics are on the clipboard.]\n");
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    format!(
        "https://github.com/JKamsker/gitcraft/issues/new?title=Bug%20report&body={}",
        encode(&body)
    )
}
fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod issue_tests {
    #[test]
    fn issue_link_is_encoded_bounded_and_preserves_full_lines() {
        let url = super::issue_url("A & B?\n".repeat(5000).as_str());
        assert!(url.starts_with("https://github.com/JKamsker/gitcraft/issues/new?"));
        assert!(url.contains("%26") && url.len() < 7200 && !url.contains('\n'));
        assert!(url.contains("clipboard"));
    }
}
