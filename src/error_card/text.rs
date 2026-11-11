//! What the card says about each problem: its title and what is going on, in plain words.

use crate::git::errors::GitProblem;

/// Where git runs, which changes what can be done about some problems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    /// Git for Windows, on this machine.
    pub windows: bool,
    /// In WSL or over SSH (always Linux there).
    pub remote: bool,
}

impl Place {
    /// Git for Windows: where `core.longpaths` and `http.sslBackend=schannel` mean something.
    pub fn git_for_windows(self) -> bool {
        self.windows && !self.remote
    }
}

/// Most file names a line-ending card lists.
const LISTED: usize = 3;

pub fn title(problem: &GitProblem) -> String {
    match problem {
        GitProblem::DubiousOwnership { .. } => {
            crate::i18n::text("interaction-error-title-trust").to_owned()
        }
        GitProblem::StaleLock { .. } => {
            crate::i18n::text("interaction-error-title-lock").to_owned()
        }
        GitProblem::LineEndings { to_crlf: true, .. } => {
            crate::i18n::text("interaction-error-title-to-crlf").to_owned()
        }
        GitProblem::LineEndings { .. } => {
            crate::i18n::text("interaction-error-title-to-lf").to_owned()
        }
        GitProblem::NameTooLong => crate::i18n::text("interaction-error-title-path").to_owned(),
        GitProblem::Certificate { .. } => {
            crate::i18n::text("interaction-error-title-certificate").to_owned()
        }
        GitProblem::Transfer { too_large: true } => {
            crate::i18n::text("interaction-error-title-too-large").to_owned()
        }
        GitProblem::Transfer { .. } => {
            crate::i18n::text("interaction-error-title-transfer").to_owned()
        }
        GitProblem::UnrelatedHistories => {
            crate::i18n::text("interaction-error-title-histories").to_owned()
        }
        GitProblem::ShallowPush => crate::i18n::text("interaction-error-title-shallow").to_owned(),
        GitProblem::SshKeyRefused { .. } => {
            crate::i18n::text("interaction-error-title-ssh").to_owned()
        }
    }
}

/// What is going on and what can be done, a paragraph each.
pub fn explanation(problem: &GitProblem, place: Place) -> Vec<String> {
    match problem {
        GitProblem::DubiousOwnership { path } => vec![
            crate::i18n::message(
                "interaction-error-trust-why",
                &[("path", path.as_str().into())],
            ),
            crate::i18n::text("interaction-error-trust-advice").to_owned(),
        ],
        GitProblem::StaleLock { lock } => vec![
            crate::i18n::message(
                "interaction-error-lock-why",
                &[("lock", file_name(lock).into())],
            ),
            crate::i18n::text("interaction-error-lock-advice").to_owned(),
        ],
        GitProblem::LineEndings { files, fatal, .. } => line_endings(files, *fatal),
        GitProblem::NameTooLong if place.git_for_windows() => {
            vec![crate::i18n::text("interaction-error-long-windows").to_owned()]
        }
        GitProblem::NameTooLong => {
            vec![crate::i18n::text("interaction-error-long-other").to_owned()]
        }
        GitProblem::Certificate { detail } => certificate(detail, place),
        GitProblem::Transfer { too_large: true } => {
            vec![crate::i18n::text("interaction-error-large-advice").to_owned()]
        }
        GitProblem::Transfer { .. } => {
            vec![crate::i18n::text("interaction-error-transfer-advice").to_owned()]
        }
        GitProblem::UnrelatedHistories => vec![
            crate::i18n::text("interaction-error-histories-why").to_owned(),
            crate::i18n::text("interaction-error-histories-advice").to_owned(),
        ],
        GitProblem::ShallowPush => {
            vec![crate::i18n::text("interaction-error-shallow-advice").to_owned()]
        }
        GitProblem::SshKeyRefused { login } => ssh_key(login.as_deref(), place),
    }
}

fn line_endings(files: &[String], fatal: bool) -> Vec<String> {
    let mut paragraphs = Vec::new();
    if !files.is_empty() {
        let shown: Vec<&str> = files.iter().take(LISTED).map(String::as_str).collect();
        let more = files.len().saturating_sub(LISTED);
        paragraphs.push(crate::i18n::message(
            "interaction-error-files",
            &[("files", shown.join(", ").into()), ("more", more.into())],
        ));
    }
    paragraphs.push(crate::i18n::text("interaction-error-autocrlf-advice").to_owned());
    paragraphs.push(crate::i18n::text("interaction-error-attributes-advice").to_owned());
    if fatal {
        paragraphs.push(crate::i18n::text("interaction-error-safecrlf-advice").to_owned());
    }
    paragraphs
}

fn certificate(detail: &str, place: Place) -> Vec<String> {
    let mut paragraphs = vec![crate::i18n::message(
        "interaction-error-tls-why",
        &[("detail", detail.into())],
    )];
    paragraphs.push(if place.git_for_windows() {
        crate::i18n::text("interaction-error-windows-certificates").to_owned()
    } else {
        crate::i18n::text("interaction-error-other-certificates").to_owned()
    });
    paragraphs
}

fn ssh_key(login: Option<&str>, place: Place) -> Vec<String> {
    let who = login.unwrap_or(crate::i18n::text("interaction-error-server"));
    let test = login.map_or_else(
        || "ssh -T <user>@<host>".to_owned(),
        |login| format!("ssh -T {login}"),
    );
    let mut paragraphs = vec![crate::i18n::message(
        "interaction-error-ssh-why",
        &[("who", who.into()), ("test", test.as_str().into())],
    )];
    if place.remote {
        paragraphs.push(crate::i18n::text("interaction-error-remote-keys").to_owned());
    }
    paragraphs
}

/// The last name of `path`.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// "3 minutes", "2 days": how long `secs` is.
pub fn age(secs: u64) -> String {
    let secs = secs.min(i64::MAX as u64);
    let (count, id) = match secs {
        0..60 => return crate::i18n::text("interaction-error-age-short").to_owned(),
        60..3_600 => (secs / 60, "interaction-error-age-minute"),
        3_600..86_400 => (secs / 3_600, "interaction-error-age-hour"),
        86_400..604_800 => (secs / 86_400, "interaction-error-age-day"),
        604_800..2_629_800 => (secs / 604_800, "interaction-error-age-week"),
        2_629_800..31_557_600 => (secs / 2_629_800, "interaction-error-age-month"),
        _ => (secs / 31_557_600, "interaction-error-age-year"),
    };
    crate::i18n::message(id, &[("count", count.into())])
}
