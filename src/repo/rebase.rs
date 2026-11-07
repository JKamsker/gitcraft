//! Interactive rebase: the commits after a base, newest on top like the graph, each picked,
//! reworded, squashed, fixed up, dropped or stopped at to be edited, and reordered (by drag
//! and drop or with the keyboard, see [`rows`]). Starting it writes git's todo list and hands
//! it over as `sequence.editor`; rewords become an `exec git commit --amend -F`.

mod rows;
mod weather;
mod weather_git;

use std::path::{Path, PathBuf};

use egui::{Stroke, Ui};

use super::widgets;
use super::{Action, RepoTab};
use crate::git::RebaseCommit;
use crate::icons;
use crate::theme::{self, Tokens};

/// A rebase's command, and the branches it moves along: (their commit now, the ref).
pub(crate) type Moving = (Vec<String>, Vec<(String, String)>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Pick,
    Reword,
    Squash,
    Fixup,
    Drop,
    /// Stop after the commit, to amend it (`edit`): Commit in the changes pane goes on.
    Edit,
}

impl Step {
    const ALL: [Step; 6] = [
        Step::Pick,
        Step::Reword,
        Step::Edit,
        Step::Squash,
        Step::Fixup,
        Step::Drop,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Step::Pick => "Pick",
            Step::Reword => "Reword",
            Step::Squash => "Squash",
            Step::Fixup => "Fixup",
            Step::Drop => "Drop",
            Step::Edit => "Edit",
        }
    }

    fn explain(self) -> &'static str {
        match self {
            Step::Pick => "Keep the commit as it is",
            Step::Reword => "Keep the changes, edit the message",
            Step::Squash => "Meld into the commit below, keeping both messages",
            Step::Fixup => "Meld into the commit below, dropping this message",
            Step::Drop => "Remove the commit and its changes",
            Step::Edit => "Stop after this commit to change it, then Continue rebase",
        }
    }
}

#[derive(Clone)]
pub(crate) struct Entry {
    pub commit: RebaseCommit,
    pub step: Step,
    /// The new message, for rewords.
    pub message: String,
}

pub(crate) struct RebaseEditor {
    /// What the commits are replayed onto (a commit or a branch); empty to rewrite from the
    /// root commit on (`--root`).
    pub onto: String,
    pub onto_label: String,
    /// Oldest first, as git lists them.
    pub entries: Vec<Entry>,
    /// HEAD when the commits were read: the todo list replaces the branch's commits with
    /// these, so one made since (a commit, a pull) would be dropped.
    pub head: String,
    /// Which editor this is (each one opened has its own): a rebase prepared for one closed
    /// since never starts.
    pub id: u64,
    /// Its rebase is being prepared (on another machine, in the background): it can't be
    /// started again, nor changed, meanwhile.
    pub starting: bool,
    /// The row the keys act on (an index into `entries`): the newest, until another is
    /// clicked.
    pub selected: Option<usize>,
    /// How tall each row (by commit) was when last drawn: rows out of sight only take their
    /// place.
    pub heights: std::collections::HashMap<String, f32>,
    /// A text field had the keyboard at the end of the last frame: the Esc that takes it away
    /// (egui does, before the frame) doesn't also close the editor.
    pub typing: bool,
    /// Every commit is made again (`--no-ff`), even one already on its parent: commits a
    /// rewrite prepared with `commit-tree` are then signed and hooked like any other.
    pub no_ff: bool,
    /// Move the branches on the commits rewritten along with them (`--update-refs`): the lower
    /// layers of a stack rebased from its top.
    pub update_refs: bool,
    weather: weather::Weather,
}

impl Clone for RebaseEditor {
    fn clone(&self) -> Self {
        Self {
            onto: self.onto.clone(),
            onto_label: self.onto_label.clone(),
            entries: self.entries.clone(),
            head: self.head.clone(),
            id: self.id,
            starting: self.starting,
            selected: self.selected,
            heights: self.heights.clone(),
            typing: self.typing,
            no_ff: self.no_ff,
            update_refs: self.update_refs,
            weather: weather::Weather::default(),
        }
    }
}

impl RebaseEditor {
    pub fn new(onto: String, onto_label: String, commits: Vec<RebaseCommit>) -> Self {
        static IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        // `git log --reverse onto..HEAD`: HEAD comes last.
        let head = commits.last().map(|c| c.hash.clone()).unwrap_or_default();
        // The newest row, at the top, takes the keys until another is clicked.
        let selected = commits.len().checked_sub(1);
        let entries = commits
            .into_iter()
            .map(|c| Entry {
                message: c.message.clone(),
                commit: c,
                step: Step::Pick,
            })
            .collect();
        Self {
            onto,
            onto_label,
            entries,
            head,
            id: IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            starting: false,
            selected,
            heights: std::collections::HashMap::new(),
            typing: false,
            no_ff: false,
            update_refs: false,
            weather: weather::Weather::default(),
        }
    }

    /// Why the commits listed are no longer the branch's, when HEAD is at `head` now.
    pub fn moved(&self, head: &str) -> Option<String> {
        (!self.head.is_empty() && self.head != head).then(|| {
            "The branch has moved since these commits were read: rebasing would drop the new ones. Cancel and start again."
                .to_owned()
        })
    }

    /// Why the rebase can't start as set up, if it can't.
    pub fn problem(&self) -> Option<String> {
        if self.entries.iter().any(|e| e.commit.merge) {
            return Some(
                "These commits include merges, which this editor would flatten. Use git rebase -i --rebase-merges in the terminal."
                    .to_owned(),
            );
        }
        let mut kept = self.entries.iter().filter(|e| e.step != Step::Drop);
        match kept.next() {
            None => {
                return Some("Keep at least one commit (or reset the branch instead).".to_owned());
            }
            Some(e) if matches!(e.step, Step::Squash | Step::Fixup) => {
                return Some(format!(
                    "{} {} has no commit below it to meld into.",
                    e.step.label(),
                    short(&e.commit.hash)
                ));
            }
            _ => {}
        }
        if self
            .entries
            .iter()
            .any(|e| e.step == Step::Reword && e.message.trim().is_empty())
        {
            return Some("A reworded commit needs a message.".to_owned());
        }
        None
    }

    fn changed(&self, e: &Entry) -> bool {
        e.step == Step::Reword && e.message.trim_end() != e.commit.message.trim_end()
    }

    /// Writes the todo list and the new messages to a fresh temporary folder and returns the
    /// git command that runs the rebase with them. For a repository in WSL or over SSH, on
    /// that machine, where git reads them.
    pub fn command(&self, repo: &Path) -> Result<Vec<String>, String> {
        self.command_with_refs(repo).map(|(command, _)| command)
    }

    /// [`Self::command`], and the branches Update dependent branches moves along: (their
    /// commit now, `refs/heads/<branch>`).
    pub fn command_with_refs(&self, repo: &Path) -> Result<Moving, String> {
        if let Some(target) = crate::remote::parse(repo) {
            return self.command_there(repo, &target);
        }
        let dir = private_dir(&std::env::temp_dir())
            .map_err(|e| format!("could not prepare the rebase: {e}"))?;
        let write = |name: &str, text: &str| -> Result<PathBuf, String> {
            let path = dir.join(name);
            std::fs::write(&path, text)
                .map_err(|e| format!("could not prepare the rebase: {e}"))?;
            Ok(path)
        };
        let mut files = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            files.push(if self.changed(e) {
                Some(write(&format!("message-{i}.txt"), &e.message)?)
            } else {
                None
            });
        }
        let refs = self.moved_refs(repo)?;
        let todo = todo(
            &self.entries,
            |i| files.get(i).cloned().flatten().map(|p| sh_path(&p)),
            &refs,
        );
        let todo_file = write("todo.txt", &todo)?;
        Ok((self.rebase_args(&sh_path(&todo_file)), refs))
    }

    /// [`Self::command`] for a repository on another machine: the files in a private folder
    /// there.
    fn command_there(&self, repo: &Path, target: &crate::remote::Target) -> Result<Moving, String> {
        let write = |name: &str, text: &str| {
            crate::remote::host_temp_file(target, name, text.as_bytes())
                .map_err(|e| format!("could not prepare the rebase: {e}"))
        };
        let mut files = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            files.push(if self.changed(e) {
                Some(write(&format!("rebase-message-{i}.txt"), &e.message)?)
            } else {
                None
            });
        }
        let refs = self.moved_refs(repo)?;
        let todo = todo(
            &self.entries,
            |i| {
                files
                    .get(i)
                    .cloned()
                    .flatten()
                    .map(|p| crate::remote::sh_quote(&p))
            },
            &refs,
        );
        let todo_file = write("rebase-todo.txt", &todo)?;
        Ok((self.rebase_args(&crate::remote::sh_quote(&todo_file)), refs))
    }

    /// The branches moved along with the commits, when Update dependent branches is ticked.
    fn moved_refs(&self, repo: &Path) -> Result<Vec<(String, String)>, String> {
        if !self.update_refs {
            return Ok(Vec::new());
        }
        let commits: Vec<&str> = self
            .entries
            .iter()
            .map(|e| e.commit.hash.as_str())
            .collect();
        dependent_refs(repo, &commits)
    }

    /// The rebase, its todo list copied from `todo_file` (quoted for `sh`).
    fn rebase_args(&self, todo_file: &str) -> Vec<String> {
        let mut args = vec![
            "-c".to_owned(),
            format!("sequence.editor=cp {todo_file}"),
            "rebase".to_owned(),
            "-i".to_owned(),
            "--autostash".to_owned(),
        ];
        if self.update_refs {
            args.push("--update-refs".to_owned());
        }
        if self.no_ff {
            args.push("--no-ff".to_owned());
        }
        if self.onto.is_empty() {
            args.push("--root".to_owned());
        } else {
            args.extend(["--end-of-options".to_owned(), self.onto.clone()]);
        }
        args
    }
}

/// A new folder under `parent` that only this user can enter. Never one that already exists:
/// in a shared temporary folder, another user could have made it to swap the todo list (whose
/// `exec` lines git runs) for their own.
fn private_dir(parent: &Path) -> std::io::Result<PathBuf> {
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let base = format!(
        "gitcraft-rebase-{}-{}",
        std::process::id(),
        crate::git::now()
    );
    let mut n = 0u32;
    loop {
        let dir = parent.join(if n == 0 {
            base.clone()
        } else {
            format!("{base}-{n}")
        });
        match builder.create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 100 => n += 1,
            Err(e) => return Err(e),
        }
    }
}

/// git's todo list for `entries` (oldest first). `message_file(i)` is the quoted path of entry
/// `i`'s new message when it was reworded.
/// `refs`: (commit, `refs/heads/<branch>`) for the branches moved along (`--update-refs`), each
/// placed after its commit, as git places them.
fn todo(
    entries: &[Entry],
    message_file: impl Fn(usize) -> Option<String>,
    refs: &[(String, String)],
) -> String {
    let mut out = String::new();
    // A branch's update-ref waits for the squashes and fixups melded into its commit: placed
    // before them, the branch would be left on the commit as it was before they were.
    let mut waiting: Vec<&str> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if !matches!(e.step, Step::Squash | Step::Fixup) {
            for name in waiting.drain(..) {
                out.push_str(&format!("update-ref {name}\n"));
            }
        }
        let verb = match e.step {
            Step::Pick | Step::Reword => "pick",
            Step::Squash => "squash",
            Step::Fixup => "fixup",
            Step::Drop => "drop",
            Step::Edit => "edit",
        };
        out.push_str(&format!("{verb} {} {}\n", e.commit.hash, e.commit.summary));
        if let Some(file) = message_file(i) {
            out.push_str(&format!(
                "exec git commit --amend --allow-empty --cleanup=whitespace -F {file}\n"
            ));
        }
        waiting.extend(
            refs.iter()
                .filter(|(at, _)| *at == e.commit.hash)
                .map(|(_, name)| name.as_str()),
        );
    }
    for name in waiting {
        out.push_str(&format!("update-ref {name}\n"));
    }
    out
}

/// The branches on `commits` that Update dependent branches moves along: (commit,
/// `refs/heads/<branch>`). Not those checked out in a worktree (this one's included), which
/// git refuses to move under them.
fn dependent_refs(repo: &Path, commits: &[&str]) -> Result<Vec<(String, String)>, String> {
    let out = crate::git::run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/heads",
        ],
    )?;
    let checked_out: Vec<String> = crate::git::worktree_list(repo, None)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|w| w.branch)
        .map(|b| format!("refs/heads/{b}"))
        .collect();
    Ok(out
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(at, name)| commits.contains(at) && !checked_out.iter().any(|c| c == name))
        .map(|(at, name)| (at.to_owned(), name.to_owned()))
        .collect())
}

/// `path` for git's shell (`sh`, also on Windows): forward slashes, single-quoted.
fn sh_path(path: &Path) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    format!("'{}'", p.replace('\'', r"'\''"))
}

/// How the rows are moved and their actions set, for the footer's tooltip.
const KEYS_HINT: &str = "Drag a row by its handle, or Alt+↑ / Alt+↓, to move it. Click a row, then P / R / E / S / F / D set its action and ↑ / ↓ select another.";

fn short(hash: &str) -> &str {
    hash.get(..7).unwrap_or(hash)
}

impl RepoTab {
    pub(super) fn rebase_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        self.poll_rebase_weather(ui.ctx());
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let branch = self
            .refs
            .current
            .clone()
            .unwrap_or_else(|| "HEAD".to_owned());
        let head = self.refs.head.clone();
        let keys_free =
            self.dialog.is_none() && !self.keys_blocked && !ui.ctx().egui_wants_keyboard_input();
        let Some(editor) = &mut self.rebase else {
            return;
        };

        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, "git-merge", 16.0, t.icon);
                    ui.label(
                        egui::RichText::new(format!(
                            "Interactive rebase · {branch} onto {}",
                            editor.onto_label
                        ))
                        .font(theme::medium(13.0)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", "Cancel (Esc)").clicked() {
                            acts.push(Action::CloseRebase);
                        }
                        if icons::small_button(ui, "refresh-cw", "Recheck rebase weather").clicked()
                        {
                            editor.weather.refresh(ui.ctx());
                        }
                    });
                });
            });

        let problem = editor.problem().or_else(|| editor.moved(&head));
        egui::Panel::bottom("rebase_footer")
            .frame(
                egui::Frame::new()
                    .fill(t.chrome)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .stroke(Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| {
                // An option of the rebase, above what starts it.
                ui.add_enabled(
                    !editor.starting,
                    egui::Checkbox::new(&mut editor.update_refs, "Update dependent branches"),
                )
                .on_hover_text(
                    "Move the branches on these commits along with them (git rebase \
                     --update-refs, git 2.38+): the lower layers of a stack rebased from its \
                     top. Branches checked out in other worktrees are left where they are; \
                     Restack… in STACKS handles those. One Ctrl+Z puts every branch back.",
                );
                ui.add_space(4.0);
                // The buttons first, from the right: the text takes what is left, cut short.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(
                        ui,
                        "Start rebase",
                        idle && problem.is_none() && !editor.starting,
                        120.0,
                    )
                    .clicked()
                    {
                        acts.push(Action::RunRebase);
                    }
                    if ui.button("Cancel").clicked() {
                        acts.push(Action::CloseRebase);
                    }
                    if editor.starting {
                        ui.add(crate::spinner::Spinner::new());
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let note = match &problem {
                            Some(p) => egui::RichText::new(p).color(t.danger),
                            None => {
                                let n = editor.entries.len();
                                egui::RichText::new(format!(
                                    "{n} commit{}, newest on top · Ctrl+Z undoes the rebase",
                                    if n == 1 { "" } else { "s" }
                                ))
                                .color(t.text_dim)
                            }
                        };
                        ui.add(egui::Label::new(note).truncate())
                            .on_hover_text(KEYS_HINT);
                    });
                });
            });

        // The row keys act on this frame, unless something else has the keyboard.
        let keys = editor.keys(ui, keys_free);
        editor.list(ui, keys);
        editor.weather.observe(&editor.entries, ui.ctx());
        editor.typing = ui.ctx().memory(|m| m.focused().is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(hash: &str, step: Step) -> Entry {
        Entry {
            commit: RebaseCommit {
                hash: hash.to_owned(),
                summary: format!("summary {hash}"),
                message: format!("summary {hash}"),
                ..RebaseCommit::default()
            },
            step,
            message: format!("summary {hash}"),
        }
    }

    /// Rebasing the top of a stack with Update dependent branches moves the layers below
    /// along; one checked out in another worktree stays (git's `--update-refs` skips it).
    #[test]
    fn update_dependent_branches_moves_the_lower_layers() -> Result<(), String> {
        let repo = crate::git::stack::test_repo::TestRepo::new("rebase update refs")?;
        repo.stack(&["schema", "api", "ui"])?;
        repo.commit("m2.txt", "2\n", "main moves on")?;
        let wt = repo.dir.with_file_name(format!(
            "{} schema wt",
            repo.dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&wt);
        let schema = repo.tip("schema")?;
        repo.checkout("ui")?;
        let result = (|| -> Result<(), String> {
            let commits = crate::git::rebase_commits(repo.path(), "main")?;
            let mut editor = RebaseEditor::new("main".into(), "main".into(), commits);
            editor.update_refs = true;
            // schema checked out in a worktree of its own: it stays.
            repo.git(&["worktree", "add", "-q", &wt.to_string_lossy(), "schema"])?;
            let command = editor.command(repo.path())?;
            let args: Vec<&str> = command.iter().map(String::as_str).collect();
            assert!(args.contains(&"--update-refs"));
            repo.git(&args)?;
            let is_ancestor =
                |a: &str, b: &str| repo.git(&["merge-base", "--is-ancestor", a, b]).is_ok();
            assert!(is_ancestor("main", "ui") && is_ancestor("main", "api"));
            assert_eq!(repo.tip("schema")?, schema);
            Ok(())
        })();
        let _ = repo.git(&["worktree", "remove", "--force", &wt.to_string_lossy()]);
        let _ = std::fs::remove_dir_all(&wt);
        result
    }

    #[test]
    fn todo_lines() {
        let entries = vec![
            entry("a1", Step::Pick),
            entry("b2", Step::Reword),
            entry("c3", Step::Squash),
            entry("d4", Step::Drop),
        ];
        // api on b2, which c3 is squashed into: placed after the squash. ui on the last one.
        let refs = [
            ("b2".to_owned(), "refs/heads/api".to_owned()),
            ("d4".to_owned(), "refs/heads/ui".to_owned()),
        ];
        let todo = todo(
            &entries,
            |i| (i == 1).then(|| "'/tmp/m 1.txt'".to_owned()),
            &refs,
        );
        assert_eq!(
            todo,
            "pick a1 summary a1\npick b2 summary b2\nexec git commit --amend --allow-empty --cleanup=whitespace -F '/tmp/m 1.txt'\nsquash c3 summary c3\nupdate-ref refs/heads/api\ndrop d4 summary d4\nupdate-ref refs/heads/ui\n"
        );
    }

    #[test]
    fn problems() {
        let mut ed = RebaseEditor::new("x".into(), "x".into(), Vec::new());
        ed.entries = vec![entry("a1", Step::Drop), entry("b2", Step::Fixup)];
        assert!(ed.problem().is_some_and(|p| p.contains("below")));
        ed.entries[1].step = Step::Pick;
        assert_eq!(ed.problem(), None);
        ed.entries[1].step = Step::Drop;
        assert!(ed.problem().is_some());
        ed.entries[1].step = Step::Reword;
        ed.entries[1].message = "  ".into();
        assert!(ed.problem().is_some());
    }

    #[test]
    fn a_branch_moved_since_the_read_is_not_rebased() {
        let commits = ["a1", "b2"].map(|h| entry(h, Step::Pick).commit).to_vec();
        let mut ed = RebaseEditor::new("x".into(), "x".into(), commits);
        assert_eq!(ed.moved("b2"), None);
        // Reordering doesn't change which commit was HEAD.
        ed.entries.swap(0, 1);
        assert_eq!(ed.moved("b2"), None);
        // A commit made since: its todo list would drop it.
        assert!(ed.moved("c3").is_some());
    }

    #[test]
    fn private_dirs_are_new() {
        let parent = std::env::temp_dir().join(format!("gitcraft-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&parent);
        let a = private_dir(&parent);
        let b = private_dir(&parent);
        assert!(a.is_ok() && b.is_ok());
        if let (Ok(a), Ok(b)) = (&a, &b) {
            assert_ne!(a, b);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(a).map(|m| m.permissions().mode() & 0o777);
                assert_eq!(mode.ok(), Some(0o700));
            }
        }
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn shell_paths() {
        assert_eq!(
            sh_path(Path::new(r"C:\Temp\it's\todo.txt")),
            r"'C:/Temp/it'\''s/todo.txt'"
        );
    }
}
