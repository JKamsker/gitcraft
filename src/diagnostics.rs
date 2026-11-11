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

mod privacy;
pub use privacy::*;

#[cfg(test)]
mod tests;

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
mod issue_tests;

#[cfg(test)]
mod privacy_regression_tests;

#[cfg(test)]
mod localization_tests;
