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

/// Render known application control errors without changing their transport identity.
/// Provider/Git diagnostics and unknown strings remain opaque.
pub fn display_error(error: &str) -> String {
    if let Some(display) = super::hooks::display_failure(error) {
        return display;
    }
    if let Some(path) = error.strip_suffix(" is not inside a git repository") {
        return crate::i18n::message(
            "backend-is-not-inside-a-git-repository-116bc9",
            &[("path", path.into())],
        );
    }
    if let Some((path, target)) = error.rsplit_once(" isn't a folder on ")
        && (target.starts_with("SSH · ") || target.starts_with("WSL · "))
    {
        return crate::i18n::message(
            "backend-isn-t-a-folder-on-859699",
            &[("path", path.into()), ("target", target.into())],
        );
    }
    if let Some(rest) = error.strip_suffix(" seconds and was stopped")
        && let Some((what, seconds)) = rest.rsplit_once(" didn't finish within ")
        && what.starts_with("git ")
        && let Ok(seconds) = seconds.parse::<u64>()
    {
        return crate::i18n::message(
            "backend-what-didn-t-finish-within-seconds-and-was-stopped-ffb894",
            &[("what", what.into()), ("seconds", seconds.into())],
        );
    }
    if let Some(detail) = error.strip_prefix("Contained bundle Git: ") {
        return crate::i18n::message(
            "backend-contained-bundle-error",
            &[("detail", display_error(detail).into())],
        );
    }
    let id = match error {
        "The operation on the previous primary forge finished; refresh that repository to check its result" => {
            "dev-baseline-guard-21"
        }
        "The primary remote changed or was removed; plan the fork action again" => {
            "dev-baseline-guard-0"
        }
        "The effective primary fetch URL changed; plan the fork action again" => {
            "dev-baseline-guard-1"
        }
        "The contributor's fork remote was removed; plan the action again" => {
            "dev-baseline-guard-2"
        }
        "The contributor's fork remote changed; plan the action again" => "dev-baseline-guard-3",
        "The primary forge changed; plan this fork action again. Any completed local rewrite remains undoable." => {
            "dev-baseline-guard-4"
        }
        "Unknown saved checkout forge" => "dev-baseline-guard-5",
        "This saved fork checkout has no captured destination; check out the pull request again" => {
            "dev-baseline-guard-6"
        }
        "The saved fork checkout has no initial effective destination; check out the pull request again" => {
            "dev-baseline-guard-7"
        }
        "The saved fork remote is missing; check out the pull request again" => {
            "dev-baseline-guard-8"
        }
        "The saved fork destination changed; check out the pull request again" => {
            "dev-baseline-guard-9"
        }
        "Missing fork push destination" => "dev-baseline-guard-10",
        "Fork push requires an explicit expected commit" => "dev-baseline-guard-11",
        "Fork push has no recorded expected commit; plan the action again" => {
            "dev-baseline-guard-12"
        }
        "The rebased branch changed before its push; plan the action again" => {
            "dev-baseline-guard-13"
        }
        "Missing fork push refspec" => "dev-baseline-guard-14",
        "This issue belongs to another primary forge; choose it again" => "dev-baseline-guard-15",
        "This issue belongs to another forge repository; open its saved URL to view it" => {
            "dev-baseline-guard-16"
        }
        "No primary forge is selected" => "dev-baseline-guard-17",
        "The primary forge remote is unavailable; refresh before opening this pull request" => {
            "dev-baseline-guard-18"
        }
        "Publication needs the release's push destination" => "dev-baseline-guard-19",
        "The primary forge changed during this release; publication stopped. Local changes remain undoable. Close this release before starting one on the new forge." => {
            "dev-baseline-guard-20"
        }
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
        "cancelled" => "backend-helper-display-8b47045eb7",
        "Cancelled" => "backend-helper-display-d353a99eb4",
        "stopped" => "backend-helper-display-8322e87d24",
        "History investigation cancelled" => "backend-helper-display-f7df6fb542",
        "DVC is not installed on this repository machine" => "backend-helper-display-219d77f375",
        "invalid resource helper arguments" => "backend-helper-display-389db1e9f2",
        "invalid keep retirement arguments" => "backend-helper-display-50ca4f5bd0",
        "invalid keep hash" => "backend-helper-display-f833fa6387",
        "invalid keep message" => "backend-helper-display-46b86a42f2",
        "Input exceeds 128 MiB" => "backend-helper-display-c1a950664d",
        "Git did not return an exit code" => "backend-helper-display-7f609a6911",
        "Unsupported resource helper platform" => "backend-helper-display-bcc9225eef",
        "Bundle unpacking requires Linux hard address-space limits on this host; no pack was opened." => {
            "backend-helper-display-286c0e8cfe"
        }
        "hard resource limit not confirmed" => "backend-helper-display-f0c58d1c8d",
        "invalid keep ownership" => "backend-helper-display-06729de987",
        "keep ownership changed; retained" => "backend-helper-display-a3db0ebabc",
        "keep identity changed; retained" => "backend-helper-display-4b3bf71f7e",
        "Safe remote staging requires Python 3 standard-library no-follow file and ACL APIs; install python3 on the repository machine" => {
            "backend-helper-display-18d2a159d4"
        }
        "Safe index reads require Perl with Fcntl no-follow support on the repository machine" => {
            "backend-helper-display-124d998466"
        }
        "File comparison requires md5sum (Linux/WSL) or md5 (macOS)" => {
            "backend-helper-display-f29ce072ac"
        }
        "Unsupported MD5 capability" => "backend-helper-display-3135c24fb2",
        "Create-only recovery needs GNU ln or Perl" => "backend-helper-display-b26048e0e2",
        "legacy or rewritten remote" => "backend-native-fallback-c3d2b99ca7",
        "remote has no fetch URL" => "backend-native-fallback-d81c296d75",
        "remote display requires Git" => "backend-native-fallback-c1ee10596f",
        "tracking graph exceeds native bound" => "backend-native-fallback-35c9ca76bd",
        "ambiguous upstream short name" => "backend-native-fallback-08b2647385",
        "symbolic reference requires Git" => "backend-native-fallback-1b16d9cfdb",
        "non-commit branch requires Git" => "backend-native-fallback-e1e6d89f7b",
        "unsupported upstream namespace" => "backend-native-fallback-850e897a1e",
        "local upstream requires Git" => "backend-native-fallback-bd13f61f25",
        "feature disabled" => "backend-native-fallback-3369979818",
        "Explicit transient sign-in requires HTTPS" => "backend-helper-display-675478fd62",
        "Use a credential-free remote URL with explicit sign-in" => {
            "backend-helper-display-24d299539e"
        }
        "Selected Git executable was not pinned" => "backend-helper-display-2c92424593",
        "Explicit transient HTTPS sign-in requires recognized Git 2.46 or newer on the repository machine; use the profile credential manager otherwise." => {
            "backend-helper-display-2287177378"
        }
        "Credential transport needs an absolute original Git exec-path" => {
            "backend-helper-display-dbf94596ef"
        }
        "Scoped sign-in is only available to the designated Git HTTP transport. LFS and credential-fill consumers must use their own explicitly selected account." => {
            "backend-helper-display-7ed9b0c2ef"
        }
        "Bundle output exceeds the size limit" => "backend-helper-display-caef832ec9",
        "base64 is missing" => "backend-helper-display-15fb4e0da4",
        _ => return error.to_owned(),
    };
    crate::i18n::text(id).to_owned()
}
