//! "What will this do": the part of a confirmation that says what a destructive operation
//! changes before it runs. The commits leaving a branch and where else each is, the
//! uncommitted files lost, whether a force push will be needed, the git commands it runs and
//! whether Ctrl+Z restores it. What comes from git is read in the background.

use std::sync::atomic::{AtomicU64, Ordering};

use egui::{RichText, Ui};

use super::{Dialog, Instead};
use crate::git::{self, Kept, LostFiles, Preview, PreviewAsk};
use crate::repo::operations::is_expectation;
use crate::repo::{RepoTab, Undo, short};
use crate::theme::{self, Tokens};

/// Tells one reading from the next: a dialog only takes the answer to its own.
static ASKED: AtomicU64 = AtomicU64::new(0);

/// Where a confirmation's preview stands.
pub(crate) enum PreviewState {
    /// Nothing asked yet (the dialog isn't open).
    Unread,
    /// Being read; the number of the reading.
    Loading(u64),
    Ready(Preview),
    Failed(String),
}

/// What a confirmation shows of what it does, and what it asks git for it.
pub(crate) struct WhatItDoes {
    pub(crate) ask: PreviewAsk,
    /// Over the commits leaving: `Leaving main`.
    pub(crate) heading: String,
    /// Over the files lost: `Uncommitted changes lost`.
    pub(crate) files_heading: String,
    pub(crate) state: PreviewState,
    /// An action to take in place of the dialog's ops, which works out its commands when it
    /// runs (a rebase's todo list), with how to show them.
    pub(crate) instead: Option<Instead>,
}

impl WhatItDoes {
    pub(crate) fn new(ask: PreviewAsk, heading: String) -> Self {
        WhatItDoes {
            ask,
            heading,
            files_heading: "Uncommitted changes lost".to_owned(),
            state: PreviewState::Unread,
            instead: None,
        }
    }
}

impl Default for WhatItDoes {
    fn default() -> Self {
        Self::new(PreviewAsk::nothing(), String::new())
    }
}

impl RepoTab {
    /// Shows `dialog`, starting to read what a confirmation's operation would change.
    pub(in crate::repo) fn open_dialog(&mut self, mut dialog: Dialog, ctx: &egui::Context) {
        if let Dialog::AddWorktree { repo, root, .. } = &mut dialog {
            *repo = self
                .worktrees
                .first()
                .map_or(&self.path, |w| &w.path)
                .clone();
            *root = if self.worktree_root.is_empty() {
                crate::remote::git_path(&git::default_worktree_root(repo))
            } else {
                self.worktree_root.clone()
            };
            dialog.suggest_path();
        }
        if let Dialog::Confirm { preview, .. } = &mut dialog {
            self.read_preview(preview, ctx);
        }
        self.dialog = Some(dialog);
    }

    /// Reads what `preview` asks for: on a background thread when it takes git (a repository
    /// on another machine can take seconds), at once when it doesn't.
    fn read_preview(&self, preview: &mut WhatItDoes, ctx: &egui::Context) {
        if !preview.ask.reads_git() {
            preview.state = PreviewState::Ready(listed_only(&preview.ask.files));
            return;
        }
        let asked = ASKED.fetch_add(1, Ordering::Relaxed) + 1;
        preview.state = PreviewState::Loading(asked);
        let repo = self.path.clone();
        let ask = preview.ask.clone();
        self.then(
            ctx,
            move || git::preview(&repo, &ask),
            move |tab, result, _| tab.preview_read(asked, result),
        );
    }

    /// The full name of the remote-tracking branch the local branch `branch` is pushed to,
    /// while it is there.
    pub(in crate::repo) fn upstream_ref(&self, branch: &str) -> Option<String> {
        let b = self.refs.local.iter().find(|b| b.name == branch)?;
        if b.upstream.is_empty() || b.upstream_gone() {
            return None;
        }
        // `%(upstream:short)` is `remotes/origin/x` when a branch is called `origin/x` too.
        let upstream = b.upstream.strip_prefix("remotes/").unwrap_or(&b.upstream);
        (self.refs.remotes.iter())
            .flat_map(|(_, branches)| branches)
            .find(|r| r.name == upstream)
            .map(|r| format!("refs/remotes/{}", r.name))
    }

    /// What deleting the refs `moving`, at `tips`, would change: the commits HEAD hasn't.
    pub(in crate::repo) fn delete_preview(
        &self,
        tips: &[&str],
        moving: Vec<String>,
    ) -> (PreviewAsk, String) {
        let heading = if self.refs.head.is_empty() {
            "Its commits".to_owned()
        } else {
            let head = self.refs.current.as_deref().unwrap_or("HEAD");
            format!("Commits not on {head}")
        };
        (PreviewAsk::delete(tips, &self.refs.head, moving), heading)
    }

    /// Puts the preview read as `asked` in the dialog, if it is still the one waiting for it.
    fn preview_read(&mut self, asked: u64, result: Result<Preview, String>) {
        let Some(Dialog::Confirm { preview, .. }) = &mut self.dialog else {
            return;
        };
        if matches!(preview.state, PreviewState::Loading(n) if n == asked) {
            preview.state = match result {
                Ok(found) => PreviewState::Ready(found),
                Err(e) => PreviewState::Failed(e),
            };
        }
    }
}

/// The preview of an operation that loses only files it names: no git to ask.
fn listed_only(files: &LostFiles) -> Preview {
    let LostFiles::Listed(paths) = files else {
        return Preview::default();
    };
    let mut paths = paths.clone();
    paths.sort_unstable();
    paths.dedup();
    Preview {
        lost_total: paths.len(),
        lost_files: paths.into_iter().take(git::PREVIEW_SHOWN).collect(),
        ..Preview::default()
    }
}

/// Draws what the operation changes, as far as it is read.
pub(super) fn show(ui: &mut Ui, preview: &WhatItDoes) {
    let t = Tokens::get(ui.ctx());
    match &preview.state {
        PreviewState::Unread => {}
        PreviewState::Loading(_) => {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new().size(14.0));
                ui.label(RichText::new("Reading what this changes…").color(t.text_dim));
            });
        }
        PreviewState::Failed(e) => {
            ui.add_space(8.0);
            ui.add(
                egui::Label::new(
                    RichText::new(format!("Couldn't read what this changes: {e}"))
                        .color(t.text_dim),
                )
                .wrap(),
            );
        }
        PreviewState::Ready(found) => {
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .auto_shrink([false, true])
                .show(ui, |ui| ready(ui, preview, found, &t));
        }
    }
}

fn ready(ui: &mut Ui, preview: &WhatItDoes, found: &Preview, t: &Tokens) {
    let heading = &preview.heading;
    if preview.ask.leaving.is_some() && found.leaving_total == 0 {
        // Said, so that an empty list doesn't read as a preview that didn't work.
        section(ui, &format!("{heading}: none"));
    }
    if found.leaving_total > 0 {
        section(ui, &format!("{heading} ({})", found.leaving_total));
        for c in &found.leaving {
            commit_row(ui, c, t);
        }
        let more = found.leaving_total.saturating_sub(found.leaving.len());
        if more > 0 {
            more_row(ui, &more_commits(more, more_only_here(found)), t);
        }
    }
    if let Some(pushed) = &found.pushed {
        ui.add_space(8.0);
        ui.add(
            egui::Label::new(
                RichText::new(pushed_note(&pushed.upstream, pushed.commits)).color(t.warning),
            )
            .wrap(),
        );
    }
    if found.lost_total > 0 {
        section(
            ui,
            &format!("{} ({})", preview.files_heading, found.lost_total),
        );
        for path in &found.lost_files {
            ui.add(egui::Label::new(RichText::new(path).font(theme::mono(11.5))).truncate());
        }
        let more = found.lost_total.saturating_sub(found.lost_files.len());
        if more > 0 {
            more_row(ui, &format!("and {more} more"), t);
        }
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).font(theme::semibold(12.0)));
    ui.add_space(2.0);
}

/// A commit leaving: its short hash, summary, and _only here_ or _still on x_ on the right.
fn commit_row(ui: &mut Ui, c: &git::LeavingCommit, t: &Tokens) {
    let (where_, color) = match &c.kept {
        Kept::OnlyHere => ("only here".to_owned(), t.danger),
        Kept::On(name) => (format!("still on {name}"), t.text_dim),
        Kept::Elsewhere => ("still on another branch".to_owned(), t.text_dim),
    };
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(short(&c.hash))
                .font(theme::mono(11.5))
                .color(t.text_faint),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(where_).color(color).font(theme::medium(11.5)));
            // The summary in what is left, from the left, cut to fit.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(c.summary.as_str()).truncate());
            });
        });
    });
}

fn more_row(ui: &mut Ui, text: &str, t: &Tokens) {
    ui.label(RichText::new(text).color(t.text_dim));
}

/// How many of the commits not listed no other ref has. Counted and listed by two runs of git,
/// between which a fetch can move refs: never below none.
fn more_only_here(found: &Preview) -> usize {
    let shown = (found.leaving.iter())
        .filter(|c| c.kept == Kept::OnlyHere)
        .count();
    found.only_here_total.saturating_sub(shown)
}

/// `and 340 more`, with how many of those no other ref has.
fn more_commits(more: usize, only_here: usize) -> String {
    match only_here {
        0 => format!("and {more} more"),
        n if n == more => format!("and {more} more, all only here"),
        n => format!("and {more} more, {n} of them only here"),
    }
}

/// The commits counted are those the operation rewrites: for a dropped commit, it and every
/// one after it, more than the list shows.
fn pushed_note(upstream: &str, commits: usize) -> String {
    let which = if commits == 1 {
        "1 commit this rewrites is".to_owned()
    } else {
        format!("{commits} commits this rewrites are")
    };
    format!("Already pushed: {which} on {upstream}, so pushing it again will need a force push.")
}

/// The commands, folded: `ops` (the expectation steps, which run no git, left out), or
/// `described` for an operation that works its commands out when it runs.
pub(super) fn commands(
    ui: &mut Ui,
    title: &str,
    ops: &[Vec<String>],
    described: Option<&[String]>,
) {
    let lines: Vec<String> = match described {
        Some(lines) => lines.to_vec(),
        None => ops
            .iter()
            .filter(|op| !is_expectation(op))
            .map(|op| command_line(op))
            .collect(),
    };
    if lines.is_empty() {
        return;
    }
    ui.add_space(8.0);
    egui::CollapsingHeader::new(RichText::new("Git commands").font(theme::medium(12.0)))
        // Folded again for another kind of dialog.
        .id_salt(("confirm-commands", title))
        .default_open(false)
        .show(ui, |ui| {
            for line in &lines {
                ui.add(egui::Label::new(RichText::new(line).font(theme::mono(11.5))).wrap());
            }
        });
}

/// `op` as one would type it: `git` and its arguments, quoted where a shell would need it.
pub(super) fn command_line(op: &[String]) -> String {
    std::iter::once("git".to_owned())
        .chain(op.iter().map(|arg| quoted(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quoted(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_alphanumeric() || "-_./:=@^~+,%".contains(c));
    if plain {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

/// Whether Ctrl+Z brings back what the operation takes, said when it does.
pub(super) fn undo_note(ui: &mut Ui, undo: &Undo, preview: &WhatItDoes) {
    if let Some(note) = undo_text(undo, may_lose_files(preview)) {
        let t = Tokens::get(ui.ctx());
        ui.add_space(8.0);
        ui.add(egui::Label::new(RichText::new(note).color(t.success)).wrap());
    }
}

/// Whether the operation loses uncommitted files, as far as is known: until read, a hard reset
/// may, and the undo note must not promise what it can't keep.
fn may_lose_files(preview: &WhatItDoes) -> bool {
    match &preview.state {
        PreviewState::Ready(found) => found.lost_total > 0,
        _ => matches!(preview.ask.files, LostFiles::HardReset(_)),
    }
}

/// What undo restores: nothing said when it doesn't; a reset's undo moves the branch back but
/// can't bring back uncommitted changes it threw away.
fn undo_text(undo: &Undo, lost_files: bool) -> Option<String> {
    let key = crate::manual::command();
    match undo {
        Undo::None => None,
        Undo::HeadMove { .. } if lost_files => Some(format!(
            "{key}+Z moves the branch back, not the uncommitted changes."
        )),
        Undo::HeadMove { .. } | Undo::Custom { .. } => Some(format!("{key}+Z restores this.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_read_as_typed() {
        let op = |args: &[&str]| args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            command_line(&op(&["reset", "--hard", "abc123"])),
            "git reset --hard abc123"
        );
        assert_eq!(
            command_line(&op(&["restore", "--", "a b.txt"])),
            "git restore -- 'a b.txt'"
        );
        assert_eq!(command_line(&op(&["x", "it's", ""])), r"git x 'it'\''s' ''");
        assert_eq!(
            command_line(&op(&["branch", "-d", "ünï"])),
            "git branch -d ünï"
        );
    }

    #[test]
    fn more_commits_say_how_many_are_only_here() {
        assert_eq!(more_commits(340, 0), "and 340 more");
        assert_eq!(more_commits(340, 340), "and 340 more, all only here");
        assert_eq!(more_commits(340, 12), "and 340 more, 12 of them only here");
    }

    #[test]
    fn the_force_push_note_counts() {
        assert_eq!(
            pushed_note("origin/main", 1),
            "Already pushed: 1 commit this rewrites is on origin/main, so pushing it again will \
             need a force push."
        );
        assert!(pushed_note("origin/main", 3).contains("3 commits this rewrites are"));
    }

    #[test]
    fn undo_is_promised_only_for_what_it_restores() {
        let key = crate::manual::command();
        assert_eq!(undo_text(&Undo::None, false), None);
        assert_eq!(
            undo_text(&Undo::HeadMove { mode: "--keep" }, false),
            Some(format!("{key}+Z restores this."))
        );
        assert_eq!(
            undo_text(&Undo::HeadMove { mode: "--keep" }, true),
            Some(format!(
                "{key}+Z moves the branch back, not the uncommitted changes."
            ))
        );
        let custom = Undo::Custom {
            undo: Vec::new(),
            redo: Vec::new(),
        };
        assert!(undo_text(&custom, true).is_some());
    }

    /// While a hard reset's preview loads, or when it couldn't be read, the files it may lose
    /// are not promised back.
    #[test]
    fn a_hard_reset_promises_no_files_back_before_it_knows() {
        let mut preview = WhatItDoes::new(
            PreviewAsk::reset("main", "h", "t", None, true),
            String::new(),
        );
        preview.state = PreviewState::Loading(1);
        assert!(may_lose_files(&preview));
        preview.state = PreviewState::Failed("offline".to_owned());
        assert!(may_lose_files(&preview));
        preview.state = PreviewState::Ready(Preview::default());
        assert!(!may_lose_files(&preview));
        let soft = WhatItDoes::new(
            PreviewAsk::reset("main", "h", "t", None, false),
            String::new(),
        );
        assert!(!may_lose_files(&soft));
    }

    /// Refs moved between counting and listing: more listed only here than counted.
    #[test]
    fn a_count_behind_the_list_does_not_underflow() {
        let only_here = git::LeavingCommit {
            hash: "a".to_owned(),
            summary: String::new(),
            kept: Kept::OnlyHere,
        };
        let found = Preview {
            leaving: vec![only_here.clone(), only_here],
            leaving_total: 30,
            only_here_total: 1,
            ..Preview::default()
        };
        assert_eq!(more_only_here(&found), 0);
    }

    #[test]
    fn named_files_need_no_git() {
        let files = LostFiles::Listed(vec!["b.txt".to_owned(), "a b.txt".to_owned()]);
        let found = listed_only(&files);
        assert_eq!(found.lost_files, ["a b.txt", "b.txt"]);
        assert_eq!(found.lost_total, 2);
        assert_eq!(listed_only(&LostFiles::None), Preview::default());
    }
}

#[cfg(test)]
#[path = "preview_tab_tests.rs"]
mod tab_tests;
