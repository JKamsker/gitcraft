//! The repository's worktrees.

use std::path::{Path, PathBuf};

use super::run;
use super::status::parse_status;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    /// Short branch name, `None` when detached.
    pub branch: Option<String>,
    pub bare: bool,
    /// Lock reason (may be empty) when locked.
    pub locked: Option<String>,
    /// Why git would prune it (its folder is gone…).
    pub prunable: Option<String>,
    /// The main worktree (the first one git lists).
    pub main: bool,
    /// The worktree this tab shows.
    pub current: bool,
    /// How many files have uncommitted changes; `None` when not known (bare, missing…).
    pub changes: Option<usize>,
}

impl Worktree {
    /// Whether its changes are still to be counted: it has a working tree that is there, not
    /// counted yet.
    pub fn needs_count(&self) -> bool {
        !self.bare && self.prunable.is_none() && self.changes.is_none()
    }

    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

/// The worktrees of `repo`'s repository, each with how many files it has changed;
/// `current_changes` is that count for `repo` itself when already known.
pub fn worktrees(repo: &Path, current_changes: Option<usize>) -> Result<Vec<Worktree>, String> {
    let mut list = worktree_list(repo, current_changes)?;
    count_changes(repo, &mut list);
    Ok(list)
}

/// [`worktrees`] without counting the changes of any but `repo` itself (`current_changes`):
/// for a repository on another machine, where counting a hundred worktrees takes a while,
/// done after (see [`count_changes`]).
pub fn worktree_list(repo: &Path, current_changes: Option<usize>) -> Result<Vec<Worktree>, String> {
    let mut list = match run(Some(repo), &["worktree", "list", "--porcelain", "-z"]) {
        Ok(out) => parse_worktrees(&out, '\0'),
        // `-z` is git 2.36+; before, one line per field (a path with a line break in it is
        // misread there).
        Err(_) => parse_worktrees(
            &run(Some(repo), &["worktree", "list", "--porcelain"])?,
            '\n',
        ),
    };
    // Elsewhere, git's paths are of that machine: in the form this app gives them.
    if let Some(target) = crate::remote::parse(repo) {
        // A `\` in a name there would read as a folder separator here (`a\..\..\x` is another
        // folder): such a worktree is left out rather than mistaken for another.
        list.retain(|wt| !wt.path.to_string_lossy().contains('\\'));
        for wt in &mut list {
            wt.path = crate::remote::encode(&target.host, &wt.path.to_string_lossy());
        }
    }
    let is_here = crate::remote::is_same_dir_as(repo);
    for wt in &mut list {
        wt.current = is_here(&wt.path);
        if wt.current {
            wt.changes = current_changes;
        }
    }
    Ok(list)
}

/// Counts the changes of the worktrees of `list` not counted yet (see [`worktrees`]).
pub fn count_changes(repo: &Path, list: &mut [Worktree]) {
    if let Some(target) = crate::remote::parse(repo) {
        remote_changes(&target, list);
        return;
    }
    // One `status` per worktree, side by side. `--no-optional-locks` so that looking doesn't take
    // the index lock from under whatever runs in that worktree.
    std::thread::scope(|scope| {
        for wt in list.iter_mut().filter(|w| w.needs_count()) {
            scope.spawn(|| {
                let args = [
                    "--no-optional-locks",
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=all",
                ];
                wt.changes = run(Some(&wt.path), &args)
                    .ok()
                    .map(|out| parse_status(&out).changed_files());
            });
        }
    });
}

/// For a repository on another machine, the changes of every worktree counted there in one
/// go: a `status` each, a few side by side, their outputs sent back one after the other
/// (`<bytes>` on a line, then the output; `-` for one that failed). One command per worktree
/// would each wait its turn for one of the few shells kept open there, and keep the tab's other
/// reads waiting too: a repository can have a hundred worktrees.
fn remote_changes(target: &crate::remote::Target, list: &mut [Worktree]) {
    use crate::remote::{self, sh_quote};
    let todo: Vec<&mut Worktree> = list.iter_mut().filter(|w| w.needs_count()).collect();
    let paths: Vec<String> = todo
        .iter()
        .filter_map(|w| remote::parse(&w.path).map(|t| sh_quote(&t.path)))
        .collect();
    if todo.is_empty() || paths.len() != todo.len() {
        return;
    }
    let script = format!(
        r#"d=$(mktemp -d) || exit 1
n=$(nproc 2>/dev/null || echo 4)
i=0
for p in {paths}; do
  {{ git -C "$p" --no-optional-locks status --porcelain=v1 -z --untracked-files=all \
    >"$d/$i" 2>/dev/null </dev/null || rm -f "$d/$i"; }} &
  i=$((i + 1))
  [ $((i % n)) -eq 0 ] && wait
done
wait
i=0
for p in {paths}; do
  if [ -f "$d/$i" ]; then wc -c <"$d/$i"; cat "$d/$i"; else echo -; fi
  i=$((i + 1))
done
rm -rf "$d""#,
        paths = paths.join(" ")
    );
    let req = remote::Request {
        script,
        env: vec![("LANGUAGE".to_owned(), "C".to_owned())],
        ..remote::Request::default()
    };
    // Not counted: the list still shows, without the numbers.
    let Ok(out) = remote::exec(&target.host, &req) else {
        return;
    };
    for (wt, output) in todo.into_iter().zip(split_counted(&out.stdout)) {
        wt.changes = output.map(|text| parse_status(&text).changed_files());
    }
}

/// The outputs of [`remote_changes`]'s script, in order: `None` for a `status` that failed.
fn split_counted(mut bytes: &[u8]) -> Vec<Option<String>> {
    let mut outputs = Vec::new();
    while let Some(nl) = bytes.iter().position(|&b| b == b'\n') {
        let head = String::from_utf8_lossy(&bytes[..nl]).trim().to_owned();
        bytes = &bytes[nl + 1..];
        if head == "-" {
            outputs.push(None);
            continue;
        }
        let Some(len) = head.parse::<usize>().ok().filter(|&n| n <= bytes.len()) else {
            break;
        };
        outputs.push(Some(String::from_utf8_lossy(&bytes[..len]).into_owned()));
        bytes = &bytes[len..];
    }
    outputs
}

/// `worktree list --porcelain -z`: `key value\0` lines, an empty line between worktrees
/// (`sep` is `\n` without `-z`).
fn parse_worktrees(out: &str, sep: char) -> Vec<Worktree> {
    let mut list: Vec<Worktree> = Vec::new();
    let mut current: Option<Worktree> = None;
    for line in out.split(sep) {
        if line.is_empty() {
            list.extend(current.take());
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key == "worktree" {
            list.extend(current.take());
            current = Some(Worktree {
                path: PathBuf::from(value),
                main: list.is_empty(),
                ..Worktree::default()
            });
            continue;
        }
        let Some(wt) = current.as_mut() else { continue };
        match key {
            "HEAD" => wt.head = value.to_owned(),
            "branch" => {
                wt.branch = Some(
                    value
                        .strip_prefix("refs/heads/")
                        .unwrap_or(value)
                        .to_owned(),
                )
            }
            "bare" => wt.bare = true,
            "locked" => wt.locked = Some(value.to_owned()),
            "prunable" => wt.prunable = Some(value.to_owned()),
            _ => {}
        }
    }
    list.extend(current);
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Outputs with line breaks and NULs in them, an empty one and a failed one, in order.
    #[test]
    fn remote_counts_come_back_in_order() {
        let bytes = b"10\n M a\x00?? b\x000\n-\n5\n M c\x00";
        assert_eq!(
            split_counted(bytes),
            [
                Some(" M a\0?? b\0".to_owned()),
                Some(String::new()),
                None,
                Some(" M c\0".to_owned()),
            ]
        );
        // Cut short: what came whole is kept.
        assert_eq!(split_counted(b"3\nab"), Vec::<Option<String>>::new());
    }

    #[test]
    fn worktree_list() {
        let out = "worktree C:/src/app\0HEAD abc\0branch refs/heads/main\0\0\
                   worktree C:/src/app-fix\0HEAD def\0detached\0locked moving disks\0\0\
                   worktree C:/gone\0HEAD 123\0branch refs/heads/old\0prunable gitdir file points to non-existent location\0\0";
        let list = parse_worktrees(out, '\0');
        assert_eq!(list.len(), 3);
        // Git before 2.36 has no `-z`: the same, a field per line.
        assert_eq!(parse_worktrees(&out.replace('\0', "\n"), '\n'), list);
        assert!(list[0].main && !list[1].main);
        assert_eq!(list[0].branch.as_deref(), Some("main"));
        assert_eq!(list[1].branch, None);
        assert_eq!(list[1].locked.as_deref(), Some("moving disks"));
        assert!(list[2].prunable.is_some());
        assert_eq!(list[1].name(), "app-fix");
    }
}
