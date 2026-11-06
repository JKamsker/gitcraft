//! Help → Copy diagnostics: what a bug report needs to know about this install, as Markdown.
//! gitcraft's version and channel, the system, the graphics adapter, the versions of git, `gh`,
//! `glab` and `tea` and whether each is signed in, and the git settings of the doctor's
//! allowlist with where they come from. Never a token, an account, a name or an email, and the
//! home folder is written `~`.

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
    let tools: Vec<crate::forge::ToolReport> = ["gh", "glab", "tea"]
        .into_iter()
        .map(crate::forge::tool_report)
        .collect();
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
    for t in tools {
        let state = match (&t.version, t.signed_in) {
            (None, _) => "not installed".to_owned(),
            (Some(v), Some(true)) => format!("{v}, signed in"),
            (Some(v), Some(false)) => format!("{v}, not signed in"),
            (Some(v), None) => format!("{v}, sign-in not known"),
        };
        line(t.tool, &state);
    }
    match settings {
        None => out.push_str("\nNo repository open: no git settings.\n"),
        Some(Err(e)) => out.push_str(&format!("\nGit settings: could not be read ({e}).\n")),
        Some(Ok(rows)) if rows.is_empty() => {
            out.push_str("\nGit settings: none of the reported ones is set.\n")
        }
        Some(Ok(rows)) => {
            out.push_str("\nGit settings in effect (value · scope · file):\n\n```\n");
            for row in rows {
                out.push_str(row);
                out.push('\n');
            }
            out.push_str("```\n");
        }
    }
    redact_home(&out, home)
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
            Some(format!(
                "{} = {} · {} · {}",
                s.key,
                winner.shown_value(),
                winner.scope.label(),
                winner.file.as_deref().unwrap_or("-")
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

/// `text` with the home folder (as written with `\` or `/`) replaced by `~`.
pub fn redact_home(text: &str, home: &str) -> String {
    let home = home.trim_end_matches(['/', '\\']);
    if home.len() < 2 {
        return text.to_owned();
    }
    let forward = home.replace('\\', "/");
    text.replace(home, "~").replace(&forward, "~")
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
             local\0file:.git/config\0credential.helper\nstore\0",
        );
        let rows = settings_rows(&all);
        assert_eq!(rows, ["pull.rebase = false · Repository · .git/config"]);
        let tools = [
            ToolReport {
                tool: "gh",
                version: Some("gh version 2.89.0".into()),
                signed_in: Some(true),
            },
            ToolReport {
                tool: "glab",
                version: None,
                signed_in: None,
            },
        ];
        let settings = Ok(vec![
            "pull.rebase = true · Global · /home/zoë/.gitconfig".to_owned(),
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
        assert!(text.contains("**glab:** not installed"));
        assert!(text.contains("pull.rebase = true · Global · ~/.gitconfig"));
        assert!(!text.contains("zoë") && !text.contains("example.com"));
        let none = report(&facts, "git", &[], None, "");
        assert!(none.contains("No repository open"));
    }

    #[test]
    fn home_is_redacted_either_way() {
        assert_eq!(
            redact_home(r"C:\Users\ana\x and C:/Users/ana/y", r"C:\Users\ana"),
            "~\\x and ~/y"
        );
        // Too short to be a home folder: nothing replaced.
        assert_eq!(redact_home("/a/b", "/"), "/a/b");
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
        assert!(text.contains("pull.ff = only · Repository"), "{text}");
        assert!(text.contains("repository on this machine"));
        Ok(())
    }
}
