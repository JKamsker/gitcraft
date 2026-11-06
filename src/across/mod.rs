//! Across repositories: what looks over many repositories at once, wherever they are (here, in
//! WSL, over SSH). The window's parts that show it are in [`crate::app`].

pub mod runner;
pub mod workspaces;

use std::path::Path;
use std::time::Duration;

/// How many repositories are looked at (or fetched) side by side. A machine elsewhere has a
/// few shells kept open for quick reads (see `remote::pool`); more at once would only queue there.
pub const AT_ONCE: usize = 6;

/// How long a look at one repository may take: a huge one's first `status` is slow, a machine
/// that went to sleep doesn't answer at all.
pub const LOOK_FOR: Duration = Duration::from_secs(60);

/// How long a fetch of one repository may take.
pub const FETCH_FOR: Duration = Duration::from_secs(600);

/// Fetches every remote of `repo`, asking nobody for a password (a workspace's fetch runs
/// unattended, like the background fetch): a remote that needs one fails with git's message.
pub fn fetch(repo: &Path) -> Result<(), String> {
    crate::git::run_unattended(Some(repo), &["fetch", "--all", "--prune", "--quiet"], None)
        .map(|_| ())
}
