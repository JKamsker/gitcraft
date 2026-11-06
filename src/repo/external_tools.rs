//! _Open in diff tool_ and _Resolve in merge tool_: a file handed to the external tool chosen
//! in Settings, in the background, and the view read again once the tool is closed.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{DiffKey, RepoTab};
use crate::git::Area;
use crate::git::external_tool::{self as tool, DiffTarget, Kind, MergeOutcome, Place};
use crate::settings::Settings;

/// The conflicted files a merge tool is open on, by repository: a second one on the same file
/// would write over what the first saves.
static MERGING: Mutex<Vec<(PathBuf, String)>> = Mutex::new(Vec::new());

/// What a file's menu offers for the diff `key` stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolItem {
    OpenInDiffTool,
    ResolveInMergeTool,
    /// Nothing to compare (an untracked file has no other side yet).
    None,
}

pub(crate) fn tool_item(key: &DiffKey) -> ToolItem {
    match key {
        DiffKey::Work(f) if f.area == Area::Conflicted => ToolItem::ResolveInMergeTool,
        DiffKey::Work(f) if f.area == Area::Untracked => ToolItem::None,
        _ => ToolItem::OpenInDiffTool,
    }
}

/// The two sides of the file `key` shows, for a diff tool. Reads git (a commit's parent, a
/// rename): not on the window's thread.
fn diff_target(repo: &Path, key: &DiffKey) -> Result<DiffTarget, String> {
    match key {
        DiffKey::Work(f) => match f.area {
            Area::Unstaged => Ok(DiffTarget::Unstaged {
                path: f.path.clone(),
            }),
            Area::Staged => Ok(DiffTarget::Staged {
                path: f.path.clone(),
                old_path: f.old_path.clone(),
            }),
            Area::Untracked => Err(format!(
                "{} isn't tracked yet: there is nothing to compare it with",
                f.path
            )),
            Area::Conflicted => Err(format!(
                "{} is conflicted: use Resolve in merge tool",
                f.path
            )),
        },
        DiffKey::Commit { hash, path } => tool::commit_target(repo, hash, path),
        DiffKey::Range { from, to, path } => {
            Ok(tool::range_target(repo, from, to.as_deref(), path))
        }
    }
}

/// Where the tool opens, for the message saying it is waited for: over SSH it runs there.
fn where_it_runs(repo: &Path) -> String {
    match tool::place_of(repo) {
        Place::Ssh(host) => format!(" on {host} (over SSH the tool runs on that machine)"),
        Place::Here | Place::Wsl => String::new(),
    }
}

impl RepoTab {
    /// Opens the file of `key` in the diff tool; the working tree is read again once it closes.
    pub(super) fn open_in_diff_tool(&mut self, key: DiffKey, ctx: &egui::Context) {
        let choice = Settings::get(ctx).diff_tool;
        let label = choice.label(Kind::Diff);
        let file = key.path().to_owned();
        self.status = Some((
            format!(
                "Showing {file} in {label}{}: the view refreshes when it closes",
                where_it_runs(&self.path)
            ),
            false,
        ));
        let repo = self.path.clone();
        self.then(
            ctx,
            move || diff_target(&repo, &key).and_then(|t| tool::open_diff(&repo, &choice, &t)),
            move |tab, result: Result<(), String>, ctx| {
                match result {
                    Ok(()) => tab.clear_waiting(&file),
                    Err(e) => tab.status = Some((e, true)),
                }
                tab.reload_status(ctx);
            },
        );
    }

    /// Opens the conflicted file `path` in the merge tool. Once it closes, the file is resolved
    /// (staged by git) if the tool reported success, and the view is read again.
    pub(super) fn resolve_in_merge_tool(&mut self, path: String, ctx: &egui::Context) {
        let choice = Settings::get(ctx).merge_tool;
        let label = choice.label(Kind::Merge);
        let entry = (self.path.clone(), path.clone());
        let taken = MERGING
            .lock()
            .map(|mut open| {
                let taken = open.contains(&entry);
                if !taken {
                    open.push(entry.clone());
                }
                taken
            })
            .unwrap_or(false);
        if taken {
            self.status = Some((
                format!("{path} is already open in a merge tool: finish there first"),
                true,
            ));
            return;
        }
        self.status = Some((
            format!(
                "Resolving {path} in {label}{}: save and close it to finish",
                where_it_runs(&self.path)
            ),
            false,
        ));
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let result = tool::resolve_conflict(&repo, &choice, &entry.1);
                if let Ok(mut open) = MERGING.lock() {
                    open.retain(|e| *e != entry);
                }
                (result, label)
            },
            move |tab, (result, label): (Result<MergeOutcome, String>, String), ctx| {
                tab.merge_tool_closed(&path, &label, result, ctx);
            },
        );
    }

    fn merge_tool_closed(
        &mut self,
        path: &str,
        label: &str,
        result: Result<MergeOutcome, String>,
        ctx: &egui::Context,
    ) {
        match result {
            Ok(MergeOutcome::Resolved) => {
                // The conflict editor of the file would show conflicts no longer there.
                if self.conflict.as_ref().is_some_and(|ed| ed.path == path) {
                    self.conflict = None;
                }
                self.status = Some((format!("Resolved {path} in {label}"), false));
            }
            Ok(MergeOutcome::Unresolved(why)) => self.status = Some((why, false)),
            Err(e) => self.status = Some((e, true)),
        }
        self.reload_status(ctx);
    }

    /// Takes out the message that a tool on `file` is waited for, if it is still the one shown.
    fn clear_waiting(&mut self, file: &str) {
        if self
            .status
            .as_ref()
            .is_some_and(|(s, error)| !error && s.starts_with(&format!("Showing {file} in ")))
        {
            self.status = None;
        }
    }
}

/// The files a merge tool is open on now (for tests).
#[cfg(test)]
pub(super) fn merging() -> Vec<(PathBuf, String)> {
    MERGING.lock().map(|open| open.clone()).unwrap_or_default()
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::{Path, PathBuf};

    use super::super::{Action, DiffKey, RepoTab};
    use super::merging;
    use crate::git::{self, Area, StatusFile};

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
            let _ = std::fs::remove_file(self.0.with_extension("sh"));
        }
    }

    fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
        git::run(Some(dir), args)
            .map(drop)
            .map_err(|e| format!("git {args:?}: {e}"))
    }

    /// A repository whose `f.txt` was changed on both sides of a merge.
    fn conflicted(name: &str) -> Result<Dir, String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let commit = |text: &str| {
            std::fs::write(dir.join("f.txt"), text).map_err(|e| e.to_string())?;
            git(&dir, &["add", "f.txt"])?;
            git(&dir, &["commit", "-q", "-m", text])
        };
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            git(&dir, args)?;
        }
        commit("base\n")?;
        git(&dir, &["checkout", "-q", "-b", "theirs"])?;
        commit("theirs\n")?;
        git(&dir, &["checkout", "-q", "main"])?;
        commit("ours\n")?;
        assert!(git::run(Some(&dir), &["merge", "theirs"]).is_err());
        Ok(Dir(dir))
    }

    /// Sets git's `<kind>.tool` to a script running `body` (`$1`, `$2`: the files).
    fn tool(dir: &Path, kind: &str, files: &str, body: &str) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.with_extension("sh");
        std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        let section = if kind == "merge" {
            "mergetool"
        } else {
            "difftool"
        };
        git(dir, &["config", &format!("{kind}.tool"), "fake"])?;
        git(
            dir,
            &[
                "config",
                &format!("{section}.fake.cmd"),
                &format!("'{}' {files}", script.display()),
            ],
        )
    }

    /// Polls the tab until `done` holds (or a few seconds pass).
    fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
        for _ in 0..400 {
            tab.poll(ctx);
            if done(tab) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        false
    }

    fn status(tab: &RepoTab) -> String {
        tab.status.clone().map(|(s, _)| s).unwrap_or_default()
    }

    #[test]
    fn the_view_is_read_again_once_the_merge_tool_closes() -> Result<(), String> {
        let repo = conflicted("merge-refresh")?;
        let go = repo.0.join("go");
        // Waits to be let go, as a tool waits for the user, then saves the merge.
        tool(
            &repo.0,
            "merge",
            "\"$MERGED\"",
            &format!(
                "while [ ! -e '{}' ]; do sleep 0.05; done; sleep 1.1; echo merged > \"$1\"",
                go.display()
            ),
        )?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(repo.0.clone(), &ctx);
        tab.reload_status(&ctx);
        assert!(wait(&mut tab, &ctx, |t| !t.work.conflicted.is_empty()));

        tab.apply(Action::ResolveInMergeTool("f.txt".to_owned()), &ctx);
        assert!(
            status(&tab).starts_with("Resolving f.txt in "),
            "{}",
            status(&tab)
        );
        assert!(merging().contains(&(repo.0.clone(), "f.txt".to_owned())));
        // A second one on the same file while it is open is refused.
        tab.apply(Action::ResolveInMergeTool("f.txt".to_owned()), &ctx);
        assert!(status(&tab).contains("already open"), "{}", status(&tab));

        std::fs::write(&go, "").map_err(|e| e.to_string())?;
        let resolved = wait(&mut tab, &ctx, |t| {
            status(t).starts_with("Resolved f.txt") && t.work.conflicted.is_empty()
        });
        assert!(resolved, "{}", status(&tab));
        assert!(tab.work.staged.iter().any(|f| f.path == "f.txt"));
        assert!(!merging().contains(&(repo.0.clone(), "f.txt".to_owned())));
        Ok(())
    }

    #[test]
    fn a_merge_tool_closed_without_saving_leaves_the_file_conflicted() -> Result<(), String> {
        let repo = conflicted("merge-cancel")?;
        tool(&repo.0, "merge", "\"$MERGED\"", "exit 0")?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(repo.0.clone(), &ctx);
        tab.apply(Action::ResolveInMergeTool("f.txt".to_owned()), &ctx);
        let told = wait(&mut tab, &ctx, |t| status(t).contains("still conflicted"));
        assert!(told, "{}", status(&tab));
        assert!(wait(&mut tab, &ctx, |t| !t.work.conflicted.is_empty()));
        Ok(())
    }

    #[test]
    fn the_view_is_read_again_once_the_diff_tool_closes() -> Result<(), String> {
        let repo = conflicted("diff-refresh")?;
        git(&repo.0, &["merge", "--abort"])?;
        std::fs::write(repo.0.join("f.txt"), "changed\n").map_err(|e| e.to_string())?;
        // The working tree's side is the file itself: the tool puts it back as committed.
        tool(
            &repo.0,
            "diff",
            "\"$LOCAL\" \"$REMOTE\"",
            "cp \"$1\" \"$2\"",
        )?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(repo.0.clone(), &ctx);
        tab.reload_status(&ctx);
        assert!(wait(&mut tab, &ctx, |t| !t.work.unstaged.is_empty()));
        let key = DiffKey::Work(StatusFile {
            path: "f.txt".to_owned(),
            old_path: None,
            code: 'M',
            area: Area::Unstaged,
        });
        tab.apply(Action::OpenInDiffTool(key), &ctx);
        assert!(
            status(&tab).starts_with("Showing f.txt in "),
            "{}",
            status(&tab)
        );
        let refreshed = wait(&mut tab, &ctx, |t| {
            t.work.unstaged.is_empty() && t.status.is_none()
        });
        assert!(refreshed, "{}", status(&tab));
        Ok(())
    }

    #[test]
    fn a_diff_tool_that_cannot_start_says_why() -> Result<(), String> {
        let repo = conflicted("diff-error")?;
        git(&repo.0, &["merge", "--abort"])?;
        git(&repo.0, &["config", "diff.tool", "vimdiff"])?;
        std::fs::write(repo.0.join("f.txt"), "changed\n").map_err(|e| e.to_string())?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(repo.0.clone(), &ctx);
        let key = DiffKey::Work(StatusFile {
            path: "f.txt".to_owned(),
            old_path: None,
            code: 'M',
            area: Area::Unstaged,
        });
        tab.apply(Action::OpenInDiffTool(key), &ctx);
        let told = wait(&mut tab, &ctx, |t| {
            t.status
                .as_ref()
                .is_some_and(|(s, error)| *error && s.contains("runs in a terminal"))
        });
        assert!(told, "{}", status(&tab));
        Ok(())
    }

    #[test]
    fn a_file_menu_offers_the_tool_its_row_can_use() -> Result<(), String> {
        use super::{ToolItem, tool_item};
        let work = |area| {
            DiffKey::Work(StatusFile {
                path: "f".to_owned(),
                old_path: None,
                code: 'M',
                area,
            })
        };
        assert_eq!(
            tool_item(&work(Area::Conflicted)),
            ToolItem::ResolveInMergeTool
        );
        assert_eq!(tool_item(&work(Area::Untracked)), ToolItem::None);
        assert_eq!(tool_item(&work(Area::Staged)), ToolItem::OpenInDiffTool);
        assert_eq!(tool_item(&work(Area::Unstaged)), ToolItem::OpenInDiffTool);
        let commit = DiffKey::Commit {
            hash: "abc".to_owned(),
            path: "f".to_owned(),
        };
        assert_eq!(tool_item(&commit), ToolItem::OpenInDiffTool);
        Ok(())
    }
}
