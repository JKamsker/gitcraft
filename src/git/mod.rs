//! Thin wrapper over the `git` command line. Every call shells out to the installed git, so
//! gitcraft behaves exactly like the user's own git (config, credentials, hooks).

mod backlinks;
mod command;
mod commit_msg;
mod commits;
mod conflict;
mod contained;
mod detached;
mod diff;
pub mod external_tool;
mod files;
mod fold;
mod hooks;
mod lfs;
mod reflog;
mod preview;
mod notes;
mod log_order;
mod refs;
pub mod simulate;
pub mod search_query;
mod signature;
mod stash;
mod remerge;
mod status;
mod submodule;
mod timed;
mod worktree;

pub use self::backlinks::*;
pub use self::command::*;
pub use self::commit_msg::*;
pub use self::commits::*;
pub use self::conflict::*;
pub use self::contained::*;
pub use self::detached::*;
pub use self::diff::*;
pub use self::files::*;
pub use self::fold::*;
pub use self::hooks::subcommand;
pub use self::lfs::*;
pub use self::reflog::*;
pub use self::preview::*;
pub use self::notes::*;
pub use self::log_order::*;
pub use self::refs::*;
pub use self::signature::*;
pub use self::stash::*;
pub use self::remerge::*;
pub use self::status::*;
pub use self::submodule::*;
pub use self::timed::*;
pub use self::worktree::*;

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

/// `git clone url dest`, run from `dest`'s parent.
/// For a `dest` in WSL or over SSH, git there clones it, from that machine's home folder.
pub fn clone(url: &str, dest: &Path, creds: Option<&Credentials>) -> Result<(), String> {
    clone_with(url, dest, creds, &[])
}

/// [`clone`] with `options` (`--no-checkout`) before the URL.
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
        if crate::remote::is_remote(path) && !e.contains("not a git repository") {
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
    let get = |args: &[&str]| run(Some(repo), args).map(|s| s.trim().to_owned()).ok();
    Identity {
        name: get(&["config", "user.name"]).unwrap_or_default(),
        email: get(&["config", "user.email"]).unwrap_or_default(),
        local: get(&["config", "--local", "user.name"]).is_some(),
        local_email: get(&["config", "--local", "user.email"]).is_some(),
    }
}

/// "3 minutes ago", "2 days ago"… for `time` seen from `now` (both Unix seconds).
pub fn relative_time(time: i64, now: i64) -> String {
    let secs = now.saturating_sub(time).max(0);
    let (n, unit) = match secs {
        0..60 => return "just now".to_owned(),
        60..3_600 => (secs / 60, "minute"),
        3_600..86_400 => (secs / 3_600, "hour"),
        86_400..604_800 => (secs / 86_400, "day"),
        604_800..2_629_800 => (secs / 604_800, "week"),
        2_629_800..31_557_600 => (secs / 2_629_800, "month"),
        _ => (secs / 31_557_600, "year"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
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
