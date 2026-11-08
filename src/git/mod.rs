//! Typed Git operations: native local reads where parity is proven, with the installed
//! Git as the compatibility backend (config, credentials, hooks and remote machines).

mod absorb;
pub(crate) mod activity;
pub(crate) mod atlas;
mod attributes;
pub mod backend;
mod backlinks;
mod behaviour;
pub(crate) use behaviour::{BehaviourCommit, behaviour_commits};
mod bisect;
mod branch_health;
pub(crate) mod bundle_helper;
mod bundles;
pub(crate) mod cleanup;
mod clone_options;
#[cfg(test)]
mod clone_tests;
pub use clone_options::*;
mod command;
mod credential_scope;
pub(crate) use command::{WorkspaceCommand, run_workspace_command, transfer_pack};
pub use credential_scope::{CREDENTIAL_HELPER, CredentialScope};
mod commit_msg;
mod commits;
mod conflict;
mod contained;
mod detached;
mod diff;
mod diff_algorithm;
pub mod doctor;
pub mod dry_run;
pub(crate) mod dvc;
pub mod errors;
pub mod external_tool;
pub(crate) mod file_comparison;
mod files;
mod fold;
pub(crate) mod growth;
mod hooks;
mod interaction;
pub(crate) mod investigations;
mod lfs;
#[cfg(test)]
pub(crate) mod lfs_fixtures;
mod lfs_locks;
mod lfs_storage;
#[cfg(test)]
pub(crate) mod lfs_test_server;
mod line_log;
mod locks;
mod machines;
pub(crate) use machines::*;
mod log_order;
mod notebook_filter;
mod notes;
mod overview;
pub(crate) mod postcards;
mod preview;
pub mod progress;
mod range_diff;
pub(crate) mod recovery;
mod redact;
mod reflog;
mod refs;
mod rehearse;
mod remerge;
pub(crate) mod repository_tools;
mod revert;
mod reviewed;
mod rewrites;
pub mod search_query;
mod signature;
pub mod simulate;
pub mod stack;
mod stash;
mod status;
mod stream;
mod submodule;
mod timed;
mod trees;
mod unity;
#[cfg(test)]
mod visualization_tests;
#[cfg(test)]
pub(crate) use self::unity::tests as unity_tests;
mod testlane;
mod worktree;
mod worktree_location;

pub use self::absorb::*;
pub use self::attributes::*;
pub use self::backlinks::*;
pub use self::bisect::*;
pub use self::branch_health::*;
pub use self::bundles::*;
pub use self::command::*;
pub use self::commit_msg::*;
pub use self::commits::*;
pub use self::conflict::*;
pub use self::contained::*;
pub use self::detached::*;
pub use self::diff::*;
pub use self::diff_algorithm::*;
pub use self::files::*;
pub use self::fold::*;
pub use self::hooks::subcommand;
pub use self::interaction::*;
pub use self::lfs::*;
pub use self::lfs_locks::*;
pub use self::lfs_storage::*;
pub use self::line_log::{LineRange, LinesAt, line_diff, line_history, unquote_path};
pub use self::locks::*;
pub use self::log_order::*;
pub use self::notebook_filter::*;
pub use self::notes::*;
pub use self::overview::*;
pub use self::preview::*;
pub use self::range_diff::*;
pub use self::redact::redact;
pub use self::reflog::*;
pub use self::refs::*;
pub use self::rehearse::*;
pub use self::remerge::*;
pub use self::revert::*;
pub use self::reviewed::*;
pub use self::rewrites::*;
pub use self::signature::*;
pub use self::stash::*;
pub use self::status::*;
pub use self::stream::*;
pub use self::submodule::*;
#[cfg(test)]
pub(crate) use self::testlane::fixture;
pub use self::testlane::*;
pub use self::timed::*;
pub use self::trees::*;
pub use self::unity::*;
pub use self::worktree::*;
pub use self::worktree_location::*;

// Explicit names for types and helpers independently added by feature stacks.
pub use self::commit_msg::Author;
pub use self::notes::notes_ref;
pub use self::rewrites::is_ancestor;
pub use self::signature::Verdict as SignatureVerdict;
pub use self::testlane::Verdict;
use std::path::{Path, PathBuf};

/// Field and record separators for `--format` strings (unit/record separator control chars).
const FS: char = '\x1f';
const RS: char = '\x1e';

/// How many commits the graph loads at a time; scrolling to the end loads the next page.
pub const LOG_PAGE: usize = 1500;

/// For `log` and `show`: `log.showSignature` would print each commit's signature check (or
/// "No signature") into the output that is parsed, and run gpg for every commit.
const NO_SIGNATURE: &str = "--no-show-signature";

/// For `log`, `show` and `stash list`: messages and names in UTF-8, as they are read, whatever
/// `i18n.logOutputEncoding` (or `i18n.commitEncoding`, its default) says. In another encoding
/// they would come out garbled, and an edited message would be saved so.
const UTF8: &str = "--encoding=UTF-8";

/// Ends the options: what follows is a revision even when it starts with `-` (a tag named
/// `--output=x` fetched from a remote, a ref from a forge's answer…), not an option.
const END_OPTS: &str = "--end-of-options";

/// Before the subcommand: the paths after `--` are file names, not patterns (a file called
/// `[ab].txt` would also match `a.txt`, one called `:x` would be `x`). Only for commands that
/// run no hooks, which would inherit it.
const LITERAL_PATHS: &str = "--literal-pathspecs";

/// For commands that don't take [`END_OPTS`] (`rev-parse` before git 2.30, `blame`): a
/// revision that would be read as an option is refused.
fn not_an_option(rev: &str) -> Result<&str, String> {
    if rev.starts_with('-') {
        Err(format!("{rev} is not a revision"))
    } else {
        Ok(rev)
    }
}

/// Clones `url` into `dest` (a folder of this machine or another one), followed through
/// `control`: its progress reported there, and stopped on a cancel. A clone stopped so leaves
/// no folder behind that wasn't there before.
pub fn clone(
    url: &str,
    dest: &Path,
    creds: Option<&Credentials>,
    control: &std::sync::Arc<OpControl>,
) -> Result<(), String> {
    let existed = dest_exists(dest);
    let result = match crate::remote::parse(dest) {
        Some(target) => {
            let (from, there) = clone_from(&target.path);
            let from = crate::remote::encode(&target.host, from);
            run_watched(
                Some(&from),
                &["clone", "--", url, &there],
                creds,
                false,
                control,
            )
        }
        None => {
            let dest = dest.to_string_lossy();
            run_watched(None, &["clone", "--", url, &dest], creds, false, control)
        }
    };
    if result.is_err() && control.is_cancelled() && existed == Some(false) {
        remove_cancelled_clone(dest);
    }
    result.map(|_| ())
}

/// Whether anything is at `dest`: a folder, a file or a link, even a broken one (`None`: it
/// couldn't be told).
fn dest_exists(dest: &Path) -> Option<bool> {
    if crate::remote::is_remote(dest) {
        let dir = crate::remote::try_is_dir(dest).ok()?;
        let file = crate::remote::try_is_file(dest).ok()?;
        let link = crate::remote::try_is_link(dest).ok()?;
        Some(dir || file || link)
    } else {
        Some(dest.symlink_metadata().is_ok())
    }
}

/// Removes what a cancelled clone left in `dest`, which wasn't there before it (git removes it
/// itself when stopped politely, but not when killed outright).
fn remove_cancelled_clone(dest: &Path) {
    match crate::remote::parse(dest) {
        // Never a home or the root, whatever went wrong before.
        Some(target) if matches!(target.path.as_str(), "" | "/" | "~" | "~/") => {}
        Some(target) => {
            let _ = crate::remote::exec(
                &target.host,
                &crate::remote::Request {
                    script: format!("rm -rf -- {}", crate::remote::sh_path(&target.path)),
                    ..Default::default()
                },
            );
        }
        None => {
            let _ = std::fs::remove_dir_all(dest);
        }
    }
}

/// Where git clones into `dest` (a path on another machine) from, and `dest` as it is given:
/// from `/` for an absolute path; under `~` as a path relative to it (git doesn't expand `~`
/// itself; only an SSH path can say `~`, a WSL one is always absolute); the home folder itself
/// as `.` there (git takes it when empty).
fn clone_from(dest: &str) -> (&'static str, String) {
    match dest.strip_prefix("~/") {
        Some(rest) => ("~", rest.to_owned()),
        None if dest == "~" => ("~", ".".to_owned()),
        None => ("/", dest.to_owned()),
    }
}

/// The folder name `git clone` would pick for `url`.
pub fn clone_dir_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches(['/', '\\']);
    // A repository's `.git` folder clones under the name of the folder it is in.
    let trimmed = trimmed
        .strip_suffix(".git")
        .filter(|rest| rest.ends_with(['/', '\\']))
        .map_or(trimmed, |rest| rest.trim_end_matches(['/', '\\']));
    let last = trimmed.rsplit(['/', '\\', ':']).next().unwrap_or_default();
    last.strip_suffix(".git").unwrap_or(last).to_owned()
}

/// `git --version`, or an error if git isn't installed / not on PATH.
pub fn version() -> Result<String, String> {
    run(None, &["--version"]).map(|s| s.trim().to_owned())
}

/// The top-level folder of the repository containing `path`.
pub fn toplevel(path: &Path) -> Result<PathBuf, String> {
    let out = run(Some(path), &["rev-parse", "--show-toplevel"]).map_err(|e| {
        // Elsewhere, not reaching the machine (or the folder there) says more.
        // Nor is a repository git refuses (owned by another user): that is explained.
        if (crate::remote::is_remote(path) && !e.contains("not a git repository"))
            || errors::is_dubious_ownership(&e)
        {
            e
        } else {
            format!("{} is not inside a git repository", path.display())
        }
    })?;
    // Only the line end: a folder's name can end with a space.
    let top = out.trim_end_matches(['\n', '\r']);
    Ok(match crate::remote::parse(path) {
        // A path of that machine, in the form this app gives them (a `\` in a name kept).
        Some(target) => crate::remote::encode(&target.host, top),
        None => PathBuf::from(top),
    })
}

pub fn init(path: &Path) -> Result<(), String> {
    crate::remote::create_dir_all(path).map_err(|e| format!("could not create folder: {e}"))?;
    run(Some(path), &["init"]).map(|_| ())
}

/// The name and email commits are made with here (a repository's own config wins over the
/// profile's, which wins over the global one), and whether this repository sets them itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub email: String,
    /// The repository sets `user.name` itself.
    pub local: bool,
    /// The repository sets `user.email` itself.
    pub local_email: bool,
}

pub fn identity(repo: &Path) -> Identity {
    let get = |local: bool| {
        let mut args = vec!["config", "-z"];
        if local {
            args.push("--local");
        }
        args.extend(["--get-regexp", "^user\\.(name|email)$"]);
        let out = run(Some(repo), &args).unwrap_or_default();
        let mut values = (None, None);
        // `key\nvalue\0`: split only the first newline, since values may contain newlines.
        // The last occurrence wins, as with `git config user.name`.
        for entry in out.split('\0') {
            let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
            match key {
                "user.name" => values.0 = Some(value.trim().to_owned()),
                "user.email" => values.1 = Some(value.trim().to_owned()),
                _ => {}
            }
        }
        values
    };
    let (name, email) = get(false);
    let (local, local_email) = get(true);
    Identity {
        name: name.unwrap_or_default(),
        email: email.unwrap_or_default(),
        local: local.is_some(),
        local_email: local_email.is_some(),
    }
}

/// "3 minutes ago", "2 days ago"… for `time` seen from `now` (both Unix seconds).
pub fn relative_time(time: i64, now: i64) -> String {
    let secs = now.saturating_sub(time).max(0);
    let (count, id) = match secs {
        0..60 => return crate::i18n::text("backend-just-now-7ddb44").to_owned(),
        60..3_600 => (secs / 60, "backend-relative-minutes"),
        3_600..86_400 => (secs / 3_600, "backend-relative-hours"),
        86_400..604_800 => (secs / 86_400, "backend-relative-days"),
        604_800..2_629_800 => (secs / 604_800, "backend-relative-weeks"),
        2_629_800..31_557_600 => (secs / 2_629_800, "backend-relative-months"),
        _ => (secs / 31_557_600, "backend-relative-years"),
    };
    crate::i18n::message(id, &[("count", count.into())])
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod extras_tests;
#[cfg(test)]
mod order_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod preview_tests;

pub fn clone_with(
    url: &str,
    dest: &Path,
    creds: Option<&Credentials>,
    options: &[&str],
) -> Result<(), String> {
    let (from, there) = match crate::remote::parse(dest) {
        Some(target) => {
            let (from, there) = clone_from(&target.path);
            (Some(crate::remote::encode(&target.host, from)), there)
        }
        None => (None, dest.to_string_lossy().into_owned()),
    };
    let mut args = vec!["clone"];
    args.extend_from_slice(options);
    args.extend(["--", url, &there]);
    run_with(from.as_deref(), &args, creds).map(|_| ())
}

pub(crate) mod drafts;

mod push_hooks;
pub(crate) use bundles::bundle_git_path;
pub(crate) use command::{PushTarget, local_repair_input, push_preflight_with, repair_host};

pub(crate) mod prompt_session;
