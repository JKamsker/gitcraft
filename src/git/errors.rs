//! Known git failures, told apart by the words of git's error output (git runs with
//! `LANGUAGE=C`, see `command.rs`, so they are English whatever the user's language), and what
//! each needs to know to be explained and fixed. The fixes themselves are in [`fix`] and
//! [`lock`].
//!
//! A refused push that isn't a fast-forward is left to the push banner.

pub mod fix;
pub mod lock;
#[cfg(test)]
mod tests;

/// Most files a line-ending warning names that are kept.
const MAX_FILES: usize = 20;

/// A failure gitcraft can explain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitProblem {
    /// The repository is owned by another user: git refuses it until `safe.directory` lists
    /// it. `path` is the value git itself says to add (`%(prefix)///server/share/x` for a
    /// network share), unquoted.
    DubiousOwnership { path: String },
    /// A `.lock` file of the repository is there already: another git is at work, or one
    /// crashed. `lock` is its path as git printed it.
    StaleLock { lock: String },
    /// git would change the line endings of `files`: `to_crlf` LF → CRLF (else CRLF → LF);
    /// `fatal` when `core.safecrlf` made it refuse rather than warn.
    LineEndings {
        files: Vec<String>,
        to_crlf: bool,
        fatal: bool,
    },
    /// A path longer than the system takes.
    NameTooLong,
    /// The HTTPS server's certificate couldn't be checked; `detail` is what TLS said.
    Certificate { detail: String },
    /// The connection broke during a transfer; `too_large` when the server answered 413.
    Transfer { too_large: bool },
    /// A merge (or pull) of two histories with no commit in common.
    UnrelatedHistories,
    /// A push from a shallow clone, which the remote doesn't take.
    ShallowPush,
    /// The SSH server took none of the keys offered; `login` is `user@host` when ssh said it.
    SshKeyRefused { login: Option<String> },
}

/// The problem `err` (git's error output) shows, if it is one gitcraft knows. The most
/// specific comes first: line-ending warnings come along with other errors, so they count only
/// when nothing else is found.
pub fn explain(err: &str) -> Option<GitProblem> {
    dubious_ownership(err)
        .or_else(|| stale_lock(err))
        .or_else(|| ssh_key_refused(err))
        .or_else(|| certificate(err))
        .or_else(|| shallow_push(err))
        .or_else(|| unrelated_histories(err))
        .or_else(|| transfer(err))
        .or_else(|| name_too_long(err))
        .or_else(|| line_endings(err))
}

/// Whether `err` is git refusing a repository owned by someone else.
pub fn is_dubious_ownership(err: &str) -> bool {
    dubious_ownership(err).is_some()
}

fn dubious_ownership(err: &str) -> Option<GitProblem> {
    if !err.contains("detected dubious ownership") && !err.contains("unsafe repository") {
        return None;
    }
    // What git says to add, else the folder it names.
    let path = err
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("git config --global --add safe.directory ")
        })
        .map(unquote)
        .or_else(|| quoted_after(err, "dubious ownership in repository at "))
        .or_else(|| {
            // 2.35.2: "unsafe repository ('/x' is owned by someone else)".
            let rest = err.split_once("unsafe repository ('")?.1;
            rest.split_once("' is owned by").map(|(p, _)| p.to_owned())
        })?;
    // Never every folder.
    (!path.is_empty() && path != "*").then_some(GitProblem::DubiousOwnership { path })
}

fn stale_lock(err: &str) -> Option<GitProblem> {
    let rest = err.split_once("Unable to create '")?.1;
    let (lock, after) = rest.split_once("': ")?;
    (lock.ends_with(".lock") && after.starts_with("File exists")).then(|| GitProblem::StaleLock {
        lock: lock.to_owned(),
    })
}

fn ssh_key_refused(err: &str) -> Option<GitProblem> {
    let line = err
        .lines()
        .find(|l| l.contains("Permission denied (") && l.contains("publickey"))?;
    let login = line
        .split_once(": Permission denied")
        .map(|(login, _)| login.trim().to_owned())
        .filter(|l| !l.is_empty() && !l.contains(' '));
    Some(GitProblem::SshKeyRefused { login })
}

fn certificate(err: &str) -> Option<GitProblem> {
    const SIGNS: [&str; 4] = [
        "SSL certificate problem",
        "server certificate verification failed",
        "certificate verify failed",
        "SEC_E_UNTRUSTED_ROOT",
    ];
    let line = err.lines().find(|l| SIGNS.iter().any(|s| l.contains(s)))?;
    // What follows "unable to access 'url': ", or the line.
    let detail = line
        .split_once("': ")
        .map_or(line, |(_, rest)| rest)
        .trim()
        .to_owned();
    Some(GitProblem::Certificate { detail })
}

fn shallow_push(err: &str) -> Option<GitProblem> {
    err.contains("shallow update not allowed")
        .then_some(GitProblem::ShallowPush)
}

fn unrelated_histories(err: &str) -> Option<GitProblem> {
    err.contains("refusing to merge unrelated histories")
        .then_some(GitProblem::UnrelatedHistories)
}

fn transfer(err: &str) -> Option<GitProblem> {
    const SIGNS: [&str; 4] = [
        "RPC failed",
        "early EOF",
        "transfer closed with",
        "unexpected disconnect while reading sideband packet",
    ];
    if !SIGNS.iter().any(|s| err.contains(s)) {
        return None;
    }
    let too_large = err.contains("HTTP 413") || err.contains("HTTP code = 413");
    Some(GitProblem::Transfer { too_large })
}

fn name_too_long(err: &str) -> Option<GitProblem> {
    // "Filename too long" on Windows, "File name too long" (ENAMETOOLONG) elsewhere.
    (err.contains("Filename too long") || err.contains("File name too long"))
        .then_some(GitProblem::NameTooLong)
}

/// What older git adds under each line-ending warning.
const ORIGINAL_ENDINGS: &str =
    "The file will have its original line endings in your working directory";

/// Line-ending warnings, or `core.safecrlf`'s refusal. Warnings alone count only when git said
/// nothing else: they come with many commands that worked, and with failures that have another
/// cause (a hook that refused a commit), which they must not be taken for.
fn line_endings(err: &str) -> Option<GitProblem> {
    let mut files = Vec::new();
    let (mut to_crlf, mut fatal, mut seen, mut other) = (true, false, false, false);
    for line in err.lines() {
        let Some((direction, file)) = line_ending_line(line) else {
            other |= !line.trim().is_empty() && !line.starts_with(ORIGINAL_ENDINGS);
            continue;
        };
        if !seen {
            to_crlf = direction;
            seen = true;
        }
        fatal |= line.starts_with("fatal:");
        if files.len() < MAX_FILES && !files.contains(&file) {
            files.push(file);
        }
    }
    (seen && (fatal || !other)).then_some(GitProblem::LineEndings {
        files,
        to_crlf,
        fatal,
    })
}

/// A line saying a file's line endings would change: (LF → CRLF, the file).
/// - 2.37 and later: `warning: in the working copy of 'x', LF will be replaced by CRLF the next
///   time Git touches it`
/// - before: `warning: LF will be replaced by CRLF in x.`
/// - `core.safecrlf`: `fatal: LF would be replaced by CRLF in x`
fn line_ending_line(line: &str) -> Option<(bool, String)> {
    let to_crlf = if line.contains("CRLF will be replaced by LF")
        || line.contains("CRLF would be replaced by LF")
    {
        false
    } else if line.contains("LF will be replaced by CRLF")
        || line.contains("LF would be replaced by CRLF")
    {
        true
    } else {
        return None;
    };
    let working_copy = line
        .split_once("in the working copy of '")
        .and_then(|(_, rest)| rest.rsplit_once("', "))
        .map(|(file, _)| file.to_owned());
    let file = working_copy.or_else(|| {
        // "replaced by CRLF in <file>.": the file is all that follows, " in " in it too.
        let (_, rest) = line
            .split_once("replaced by CRLF in ")
            .or_else(|| line.split_once("replaced by LF in "))?;
        let rest = rest.trim_end();
        Some(rest.strip_suffix('.').unwrap_or(rest).to_owned())
    })?;
    (!file.is_empty()).then_some((to_crlf, file))
}

/// The text between quotes right after `marker` in `text`: up to the last quote of its line,
/// as names can hold quotes.
fn quoted_after(text: &str, marker: &str) -> Option<String> {
    let rest = text.split_once(marker)?.1.strip_prefix('\'')?;
    let line = rest.lines().next()?;
    line.rfind('\'').map(|end| line[..end].to_owned())
}

/// A value as git quotes it for a shell (`'a b'`, `'it'\''s'`, `'a'\!'b'`), or as it is.
fn unquote(value: &str) -> String {
    let value = value.trim();
    match value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        Some(inner) => inner.replace("'\\''", "'").replace("'\\!'", "!"),
        None => value.to_owned(),
    }
}

/// Present only exact application-owned errors; preserve the raw error for classifiers.
pub fn display_error(error: &str) -> String {
    let id = match error {
        "Account action cancelled." => "backend-account-action-cancelled-13efa7",
        "Account check cancelled." => "backend-account-check-cancelled-34de37",
        "Analysis cancelled" => "backend-analysis-cancelled-d5fe01",
        "Atlas cancelled" => "backend-atlas-cancelled-b9165d",
        "Bounded forge history lookup failed or was cancelled" => {
            "backend-bounded-forge-history-lookup-failed-or-was-cancelled-41e4a7"
        }
        "Cancelled before this destination" => "backend-cancelled-before-this-destination-e79896",
        "Comparison cancelled" => "backend-comparison-cancelled-fe0a3c",
        "Content recovery cancelled or timed out" => {
            "backend-content-recovery-cancelled-or-timed-out-b878c7"
        }
        "Draft cancelled" => "backend-draft-cancelled-426ba2",
        "Draft permissions cancelled" => "backend-draft-permissions-cancelled-52db5c",
        "Export cancelled" => "backend-export-cancelled-dbef83",
        "File preview cancelled" => "backend-file-preview-cancelled-1db629",
        "File recovery cancelled or exceeds 2 MiB" => {
            "backend-file-recovery-cancelled-or-exceeds-2-mib-2d111b"
        }
        "Forge history response exceeded its budget or was cancelled" => {
            "backend-forge-history-response-exceeded-its-budget-or-was-cancelled-233e64"
        }
        "Frame cancelled" => "backend-frame-cancelled-4d6f8f",
        "GitLab operation cancelled" => "backend-gitlab-operation-cancelled-d11bad",
        "History evidence request budget reached or cancelled" => {
            "backend-history-evidence-request-budget-reached-or-cancelled-8d8d5d"
        }
        "History lookup cancelled" => "backend-history-lookup-cancelled-9533a8",
        "Ignore edit cancelled before opening" => {
            "backend-ignore-edit-cancelled-before-opening-012613"
        }
        "Ignore edit cancelled before publication" => {
            "backend-ignore-edit-cancelled-before-publication-80ba37"
        }
        "Ignore preparation cancelled" => "backend-ignore-preparation-cancelled-a562fd",
        "Ignore preview cancelled" => "backend-ignore-preview-cancelled-d6374a",
        "Line history cancelled or exceeded120seconds" => {
            "backend-line-history-cancelled-or-exceeded120seconds-7448a3"
        }
        "Not run: cancelled" => "backend-not-run-cancelled-02b0ef",
        "Object inspection cancelled" => "backend-object-inspection-cancelled-e67f16",
        "Provider request cancelled" => "backend-provider-request-cancelled-54eb9a",
        "Provider request cancelled; refresh to establish any write outcome" => {
            "backend-provider-request-cancelled-refresh-to-establish-any-write-outcome-70b94f"
        }
        "Public-key upload cancelled before sending." => {
            "backend-public-key-upload-cancelled-before-sending-eecb8c"
        }
        "Push rejected, cancelled or outcome uncertain. Inspect the destination before queueing again." => {
            "backend-push-rejected-cancelled-or-outcome-uncertain-inspect-the-destinat-78d1f3"
        }
        "Read cancelled" => "backend-read-cancelled-045da9",
        "Recovery cancelled" => "backend-recovery-cancelled-d0dd9f",
        "Recovery cancelled before mutation" => "backend-recovery-cancelled-before-mutation-922fff",
        "Replay cancelled" => "backend-replay-cancelled-c6ccd5",
        "Review cancelled" => "backend-review-cancelled-519921",
        "Saved graph view cancelled" => "backend-saved-graph-view-cancelled-754f0e",
        "Setup operation cancelled" => "backend-setup-operation-cancelled-cdccbd",
        "Symbol history cancelled or reached its time budget" => {
            "backend-symbol-history-cancelled-or-reached-its-time-budget-6f2781"
        }
        "Symbol parsing cancelled" => "backend-symbol-parsing-cancelled-9fccaf",
        "Symbol parsing cancelled or exceeded structural budget" => {
            "backend-symbol-parsing-cancelled-or-exceeded-structural-budget-b3ef57"
        }
        "Symbol parsing cancelled or exceeded two seconds" => {
            "backend-symbol-parsing-cancelled-or-exceeded-two-seconds-901d02"
        }
        "Working-folder publication cancelled" => {
            "backend-working-folder-publication-cancelled-a8f541"
        }
        _ => return error.to_owned(),
    };
    crate::i18n::text(id).to_owned()
}
