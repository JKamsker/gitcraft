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
    let system = system();
    let panic = crash::last();
    let recent = crash::recent();
    report_body(
        facts,
        git,
        tools,
        settings,
        home,
        (&system, panic.as_deref(), &recent),
    )
}

fn report_body(
    facts: &Facts,
    git: &str,
    tools: &[crate::forge::ToolReport],
    settings: Option<&Result<Vec<String>, String>>,
    home: &str,
    evidence: (&str, Option<&str>, &[String]),
) -> String {
    let (system, panic, recent) = evidence;
    let mut out = format!(
        "### {}\n\n",
        crate::i18n::text("interaction-diagnostics-title")
    );
    let mut line = |label: &str, value: &str| out.push_str(&format!("- **{label}:** {value}\n"));
    line(
        "gitcraft",
        &crate::i18n::message(
            "interaction-diagnostics-channel",
            &[
                ("version", crate::update::CURRENT.into()),
                ("channel", facts.channel.as_str().into()),
            ],
        ),
    );
    line(crate::i18n::text("interaction-diagnostics-system"), system);
    line(
        crate::i18n::text("interaction-diagnostics-graphics"),
        facts
            .adapter
            .as_deref()
            .unwrap_or(crate::i18n::text("interaction-diagnostics-unknown-adapter")),
    );
    line("git", &display_git(git));
    if let Some((active, queued)) = crate::tasks::counts() {
        line(
            crate::i18n::text("interaction-diagnostics-jobs"),
            &crate::i18n::message(
                "interaction-diagnostics-jobs-count",
                &[
                    ("active", active.into()),
                    ("workers", crate::tasks::WORKERS.into()),
                    ("queued", queued.into()),
                    ("capacity", crate::tasks::QUEUE_CAPACITY.into()),
                    ("peak", crate::tasks::peak_queued().unwrap_or(0).into()),
                ],
            ),
        );
    }
    if let Some(counts) = crate::processes::counts() {
        line(
            crate::i18n::text("interaction-diagnostics-processes"),
            &crate::i18n::message(
                "interaction-diagnostics-process-count",
                &[
                    ("active", counts.active.into()),
                    ("queued", counts.queued.into()),
                    ("started", counts.spawned.into()),
                    ("peak", counts.peak.into()),
                ],
            ),
        );
    }
    let operations = crate::git::backend::metrics();
    let native: u64 = operations.iter().map(|m| m.native_calls).sum();
    let fallback: u64 = operations.iter().map(|m| m.fallback_calls).sum();
    let cli: u64 = operations.iter().map(|m| m.subprocesses).sum();
    line(
        crate::i18n::text("interaction-diagnostics-backend"),
        &crate::i18n::message(
            "interaction-diagnostics-backend-count",
            &[
                ("native", native.into()),
                ("fallback", fallback.into()),
                ("cli", cli.into()),
            ],
        ),
    );
    for operation in operations.iter().filter(|m| m.fallback_calls > 0) {
        let reasons = operation
            .fallback_reasons
            .iter()
            .map(|(reason, count)| format!("{reason}: {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        line(
            &crate::i18n::message(
                "interaction-diagnostics-backend-fallback",
                &[("operation", operation.operation.as_str().into())],
            ),
            &reasons,
        );
    }
    for t in tools {
        // glab can only tell by asking its server: offline reads as a failed check.
        let state = match (&t.version, t.signed_in) {
            (None, _) => crate::i18n::text("interaction-diagnostics-missing-tool").to_owned(),
            (Some(version), signed) => {
                let id = match signed {
                    Some(true) if t.tool == "glab" => "interaction-diagnostics-auth-ok",
                    Some(false) if t.tool == "glab" => "interaction-diagnostics-auth-failed",
                    Some(true) => "interaction-diagnostics-signed-in",
                    Some(false) => "interaction-diagnostics-signed-out",
                    None => "interaction-diagnostics-auth-unknown",
                };
                crate::i18n::message(id, &[("version", version.as_str().into())])
            }
        };
        line(t.tool, &state);
    }
    match settings {
        None => out.push_str(&format!(
            "\n{}\n",
            crate::i18n::text("interaction-diagnostics-no-repo")
        )),
        // The error names paths and machines: left out.
        Some(Err(_)) => out.push_str(&format!(
            "\n{}\n",
            crate::i18n::text("interaction-diagnostics-settings-failed")
        )),
        Some(Ok(rows)) if rows.is_empty() => out.push_str(&format!(
            "\n{}\n",
            crate::i18n::text("interaction-diagnostics-settings-empty")
        )),
        Some(Ok(rows)) => {
            out.push_str(&format!(
                "\n{}\n\n```\n",
                crate::i18n::text("interaction-diagnostics-settings-heading")
            ));
            for row in rows {
                out.push_str(&display_setting_row(row));
                out.push('\n');
            }
            out.push_str("```\n");
        }
    }
    if let Some(panic) = panic {
        out.push_str(&format!(
            "\n{}\n\n",
            crate::i18n::text("interaction-diagnostics-panic-heading")
        ));
        out.push_str(panic);
        out.push('\n');
    }
    out.push_str(&format!(
        "\n{}\n",
        crate::i18n::text("interaction-diagnostics-recent-heading")
    ));
    for row in recent {
        out.push_str("- ");
        out.push_str(row);
        out.push('\n');
    }
    sanitize_report(&redact_users(&redact_home(&out, home)))
}

fn display_git(raw: &str) -> String {
    raw.strip_prefix("not found (")
        .and_then(|s| s.strip_suffix(')'))
        .map(|detail| {
            crate::i18n::message(
                "interaction-diagnostics-git-missing",
                &[("detail", crate::git::display_error(detail).into())],
            )
        })
        .unwrap_or_else(|| raw.to_owned())
}

fn display_setting_row(raw: &str) -> String {
    let id = match raw {
        "# repository on this machine" => Some("interaction-diagnostics-repository-here"),
        "# repository on another machine (WSL or SSH)" => {
            Some("interaction-diagnostics-repository-there")
        }
        _ => None,
    };
    if let Some(id) = id {
        return crate::i18n::text(id).to_owned();
    }
    let Some((key, rest)) = raw.split_once(" = ") else {
        return raw.to_owned();
    };
    let Some(setting) = SETTINGS
        .iter()
        .find(|s| s.key == key && !PERSONAL.iter().any(|p| p.eq_ignore_ascii_case(key)))
    else {
        return raw.to_owned();
    };
    let parts: Vec<_> = rest.split(" · ").collect();
    let [value, scope, file] = parts.as_slice() else {
        return raw.to_owned();
    };
    let value = if *value == "(configured; value withheld)" {
        crate::i18n::text("interaction-diagnostics-withheld-value")
    } else if setting.choices.contains(value) {
        *value
    } else {
        return raw.to_owned();
    };
    let scope = match *scope {
        "System" => crate::i18n::text("interaction-diagnostics-scope-system"),
        "Global" => crate::i18n::text("interaction-diagnostics-scope-global"),
        "Repository" => crate::i18n::text("interaction-diagnostics-scope-local"),
        "Worktree" => crate::i18n::text("interaction-diagnostics-scope-worktree"),
        "Command line" => crate::i18n::text("interaction-diagnostics-scope-command"),
        _ => return raw.to_owned(),
    };
    let file = match *file {
        "(origin filename withheld)" => crate::i18n::text("interaction-diagnostics-withheld-file"),
        "config" | ".gitconfig" | "gitconfig" => *file,
        _ => return raw.to_owned(),
    };
    format!("{key} = {value} · {scope} · {file}")
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
            let raw = winner.shown_value();
            // Command/program/path/free-text fields may embed credentials even without
            // a slash. Only validated finite choices may leave this machine as values.
            let value = s
                .choices
                .iter()
                .copied()
                .find(|choice| raw.eq_ignore_ascii_case(choice))
                .unwrap_or("(configured; value withheld)");
            let file = winner
                .file
                .as_deref()
                .and_then(|f| f.rsplit(['/', '\\']).next())
                .filter(|name| matches!(*name, "config" | ".gitconfig" | "gitconfig"))
                .unwrap_or("(origin filename withheld)");
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
#[cfg(any(target_os = "linux", test))]
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
                "core.editor = (configured; value withheld) · Global · .gitconfig"
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
        assert!(text.contains("a WSL path"), "{text}");
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

/// Apply to the complete clipboard/review/outgoing report, not only config rows.
pub(crate) fn sanitize_report(text: &str) -> String {
    crash::scrub(&redact_users(text))
}

/// A bounded prefilled issue URL. Full diagnostics remain on the clipboard.
pub(crate) fn issue_url(report: &str) -> String {
    let mut body = format!(
        "{}\n\n",
        crate::i18n::text("interaction-diagnostics-issue-prompt")
    );
    let report = sanitize_report(report);
    for line in report.lines() {
        if encode(&(body.clone() + line)).len() > 6500 {
            body.push_str(&format!(
                "\n{}\n",
                crate::i18n::text("interaction-diagnostics-issue-rest")
            ));
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    format!(
        "https://github.com/JKamsker/gitcraft/issues/new?title={}&body={}",
        encode(crate::i18n::text("interaction-diagnostics-issue-title")),
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

#[cfg(test)]
mod privacy_regression_tests {
    use super::*;
    #[test]
    fn command_values_and_invalid_enum_values_cannot_enter_an_issue_query() {
        let all = config::parse_origins(
            "local\0file:/owned/config\0core.editor\nenv API_TOKEN=FIXTURE_SECRET nvim\0local\0file:/owned/config\0pull.rebase\nFIXTURE_ENUM_SECRET\0local\0file:/owned/config\0gpg.program\nenv SIGNING_TOKEN=FIXTURE_SIGNING_SECRET gpg\0",
        );
        let rows = settings_rows(&all).join("\n");
        for secret in [
            "FIXTURE_SECRET",
            "FIXTURE_ENUM_SECRET",
            "FIXTURE_SIGNING_SECRET",
            "nvim",
            "env ",
        ] {
            assert!(!rows.contains(secret));
            assert!(!issue_url(&rows).contains(secret));
        }
        assert!(rows.contains("core.editor = (configured; value withheld)"));
        assert!(!issue_url("panic token=FIXTURE_PANIC_SECRET").contains("FIXTURE_PANIC_SECRET"));
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;

    #[test]
    fn actual_supplied_report_body_localizes_statuses_and_preserves_raw_rows_logs_and_redaction() {
        let facts = Facts {
            channel: "dev".into(),
            adapter: Some("opaque GPU 日本語".into()),
            repo: None,
        };
        let origins = config::parse_origins(
            "global\0file:/home/fixture/.gitconfig\0core.editor\nenv TOKEN=FIXTURE_SECRET editor\0local\0file:.git/config\0pull.rebase\nfalse\0",
        );
        let mut rows = settings_rows(&origins);
        rows.insert(0, "# repository on this machine".into());
        let original_rows = rows.clone();
        let settings = Ok(rows);
        let tools = [
            crate::forge::ToolReport {
                tool: "gh",
                version: Some("gh version opaque".into()),
                signed_in: Some(true),
            },
            crate::forge::ToolReport {
                tool: "glab",
                version: Some("glab opaque".into()),
                signed_in: Some(false),
            },
            crate::forge::ToolReport {
                tool: "tea",
                version: None,
                signed_in: None,
            },
            crate::forge::ToolReport {
                tool: "extra",
                version: Some("opaque version".into()),
                signed_in: None,
            },
        ];
        let panic = "raw log marker /home/fixture/private token=FIXTURE_TOKEN";
        let recent = vec!["git push [arguments and paths redacted] · exit 0 · 12 ms".to_owned()];
        crate::i18n::set_language("de");
        let de = report_body(
            &facts,
            "git version opaque",
            &tools,
            Some(&settings),
            "/home/fixture",
            ("recorded system", Some(panic), &recent),
        );
        for text in [
            "### gitcraft-Diagnose",
            "Kanal dev",
            "Grafik",
            "opaque GPU 日本語",
            "gh version opaque, angemeldet",
            "glab opaque, Prüfung des Anmeldestatus fehlgeschlagen",
            "**tea:** nicht gefunden (oder keine Antwort)",
            "opaque version, Anmeldung unbekannt",
            "# Repository auf diesem Rechner",
            "core.editor = (konfiguriert; Wert zurückgehalten) · Global · .gitconfig",
            "pull.rebase = false · Repository · config",
            "Letzte Panic",
            "Letzte Vorgänge",
            "git push [arguments and paths redacted] · exit 0 · 12 ms",
        ] {
            assert!(de.contains(text), "missing {text}: {de}");
        }
        for secret in [
            "FIXTURE_SECRET",
            "FIXTURE_TOKEN",
            "/home/fixture",
            "env TOKEN=",
        ] {
            assert!(!de.contains(secret), "{de}");
        }
        assert_eq!(settings.as_ref().ok(), Some(&original_rows));
        assert_eq!(tools[0].version.as_deref(), Some("gh version opaque"));
        let empty = report_body(
            &facts,
            "not found (opaque external detail)",
            &[],
            None,
            "",
            ("recorded system", None, &[]),
        );
        assert!(empty.contains("nicht gefunden (opaque external detail)"));
        assert!(empty.contains("Kein Repository geöffnet: keine Git-Einstellungen."));
        let failed = Err("ssh://user@host/private/FIXTURE_UNKNOWN".to_owned());
        let failure = report_body(
            &facts,
            "git",
            &[],
            Some(&failed),
            "",
            ("recorded system", None, &[]),
        );
        assert!(failure.contains("Git-Einstellungen konnten nicht gelesen werden."));
        assert!(!failure.contains("FIXTURE_UNKNOWN"));
        let no_rows = Ok(vec![]);
        assert!(
            report_body(
                &facts,
                "git",
                &[],
                Some(&no_rows),
                "",
                ("recorded system", None, &[])
            )
            .contains("Keine der aufgeführten Einstellungen ist gesetzt.")
        );
        crate::i18n::set_language("en-US");
        let en = report_body(
            &facts,
            "git version opaque",
            &tools,
            Some(&settings),
            "/home/fixture",
            ("recorded system", None, &recent),
        );
        assert!(en.starts_with("### gitcraft diagnostics\n\n"));
        assert!(en.contains("dev channel"));
        assert!(en.contains("gh version opaque, signed in"));
        for row in &original_rows {
            assert_eq!(display_setting_row(row), *row);
        }
        for unknown in [
            "opaque external row 日本語",
            "core.editor = unknown · Global · .gitconfig",
            "core.editor = (configured; value withheld) · Unknown · .gitconfig",
        ] {
            assert_eq!(display_setting_row(unknown), unknown);
        }
    }

    #[test]
    fn actual_issue_url_keeps_protocol_and_encoded_data_while_localizing_human_title_prompt_and_limit_notice()
     {
        crate::i18n::set_language("de");
        let report = "opaque 日本語 & ?\n".repeat(5000);
        let original = report.clone();
        let url = issue_url(&report);
        assert!(url.starts_with(
            "https://github.com/JKamsker/gitcraft/issues/new?title=Fehlerbericht&body="
        ));
        assert!(url.contains(&encode(
            "Bitte beschreibe, was passiert ist und wie es sich reproduzieren lässt."
        )));
        assert!(url.contains(&encode(
            "[Die übrigen Diagnosedaten befinden sich in der Zwischenablage.]"
        )));
        assert!(url.contains("%26") && url.len() < 7200 && !url.contains('\n'));
        assert_eq!(report, original);
        crate::i18n::set_language("en-US");
        let en = issue_url("opaque & ?");
        assert!(en.contains("title=Bug%20report&body=Please%20describe"));
        assert!(en.contains("opaque%20%26%20%3F"));
    }
}
