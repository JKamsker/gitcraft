//! Whether an attempt builds: a command run in its worktree, in a terminal where its output
//! shows, its exit status left in a file of the repository's git folder for the grid to read.

use std::path::{Path, PathBuf};

use crate::agent::sh;

/// The build command last used, kept for the repository.
pub(crate) const COMMAND_FILE: &str = "bakeoff-build.json";

/// How a build of an attempt went, as far as is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Built {
    Running,
    Passed,
    Failed(i32),
}

/// The file the build of the worktree `worktree` leaves its exit status in (for `repo`'s
/// repository): one per worktree folder.
pub(crate) fn status_file(repo: &Path, worktree: &Path) -> Result<PathBuf, String> {
    let name: String = crate::remote::git_path(worktree)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let digest = format!("{:x}", md5::compute(crate::remote::git_path(worktree)));
    let short: String = name
        .chars()
        .rev()
        .take(40)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    crate::agent::repo_file(repo, &format!("bakeoff/{short}-{}.status", &digest[..8]))
}

/// `status` as `sh` of its machine names it: a path there; here, with `/` (Git's `sh` on
/// Windows takes `C:/x/y`).
pub(crate) fn for_sh(status: &Path) -> String {
    let there = crate::remote::git_path(status);
    if crate::remote::is_remote(status) {
        there
    } else {
        there.replace('\\', "/")
    }
}

/// What runs `command` with `sh -e` in the worktree and writes its exit status to `status`
/// (a path of the repository's machine, as git there names it).
pub(crate) fn script(command: &str, status: &str) -> String {
    let dir = crate::remote::parent_there(status);
    let mut s = sh::assign_verbatim("gitcraft_build", command.trim_end());
    s.push_str(&format!(
        "mkdir -p -- {dir} && rm -f -- {status}\n\
         gitcraft_say 'building (bake-off):'\n\
         printf '%s\\n' \"$gitcraft_build\" | sed 's/^/  $ /'\n\
         sh -e -c \"$gitcraft_build\"\n\
         gitcraft_code=$?\n\
         printf '%s\\n' \"$gitcraft_code\" > {status}\n\
         if [ \"$gitcraft_code\" = 0 ]; then gitcraft_say 'it builds'; \
         else gitcraft_say \"the build failed (exit status $gitcraft_code)\"; fi",
        dir = sh::quote(dir),
        status = sh::quote(status),
    ));
    s
}

/// What a status file says: `None` when it isn't there yet (still running).
pub(crate) fn read_status(text: Option<&str>) -> Built {
    match text.map(str::trim).map(str::parse::<i32>) {
        Some(Ok(0)) => Built::Passed,
        Some(Ok(code)) => Built::Failed(code),
        _ => Built::Running,
    }
}
