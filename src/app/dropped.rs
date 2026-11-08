//! Files and folders dropped on the window: a folder opens as a repository, a patch is applied
//! to the repository tab it was dropped on, one at a time in the order dropped.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Align2, Color32, Id, LayerId, Order, Stroke};

use super::home::AppError;
use super::panels::Toast;
use super::{App, Tab, repo_root};
use crate::repo::RepoTab;
use crate::theme::{self, Tokens};

/// The extensions of the patch files a drop applies (as _Apply patch_ offers them).
const PATCH_EXTENSIONS: [&str; 4] = ["patch", "diff", "eml", "mbox"];

/// What a dropped item is, for what to do with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DropKind {
    Folder,
    Patch,
    Other,
}

/// What `path` is: a folder (`is_dir`), a patch by its extension (any case), or anything else.
pub(super) fn classify(path: &Path, is_dir: bool) -> DropKind {
    if is_dir {
        return DropKind::Folder;
    }
    if has_patch_extension(path) {
        DropKind::Patch
    } else {
        DropKind::Other
    }
}

fn has_patch_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    PATCH_EXTENSIONS.contains(&ext.as_str())
}

/// What the dropped `path` is. One on another machine (a `\\wsl.localhost\…` path) is told by
/// its name only, never by asking that machine from the window's thread: a patch by its
/// extension, anything else a folder (opening it says if it isn't one).
pub(super) fn kind_of(path: &Path) -> DropKind {
    if crate::remote::is_remote(path) {
        return if has_patch_extension(path) {
            DropKind::Patch
        } else {
            DropKind::Folder
        };
    }
    classify(path, path.is_dir())
}

/// The name of a dropped item, for a message.
pub(super) fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// What to say when a dropped folder can't be opened: git's reason, or plainly that it isn't in
/// a repository.
pub(super) fn retained_folder_error(folder: &Path, err: &str) -> AppError {
    let lower = err.to_lowercase();
    AppError::folder(
        folder,
        err,
        lower.contains("not a git repository") || lower.contains("not inside a git repository"),
    )
}

#[cfg(test)]
fn folder_error(folder: &Path, err: &str) -> String {
    retained_folder_error(folder, err).delivery_text()
}

/// The repository tab of `repo` among `tabs`.
fn repo_tab<'a>(tabs: &'a mut [&mut Tab], repo: &Path) -> Option<&'a mut RepoTab> {
    tabs.iter_mut().find_map(|t| match &mut **t {
        Tab::Repo(r) if r.path == repo => Some(&mut **r),
        _ => None,
    })
}

/// The dropped patches, each with the repository whose tab it was dropped on, applied one at a
/// time; and the one being applied.
#[derive(Default)]
pub(super) struct PatchQueue {
    waiting: VecDeque<(PathBuf, PathBuf)>,
    current: Option<(PathBuf, PathBuf)>,
}

/// The queue's session boundary also permits recording-only continuation tests.
trait PatchTargets {
    fn busy(&mut self, repo: &Path) -> Option<bool>;
    fn outcome(&mut self, repo: &Path) -> Option<Result<(), String>>;
    fn apply(&mut self, repo: &Path, file: &Path) -> Result<(), String>;
}

struct LiveTargets<'a, 'b> {
    tabs: &'a mut [&'b mut Tab],
    ctx: &'a egui::Context,
}

impl PatchTargets for LiveTargets<'_, '_> {
    fn busy(&mut self, repo: &Path) -> Option<bool> {
        repo_tab(self.tabs, repo).map(|tab| tab.dropped_patch_waits())
    }
    fn outcome(&mut self, repo: &Path) -> Option<Result<(), String>> {
        repo_tab(self.tabs, repo).and_then(RepoTab::take_dropped_patch_outcome)
    }
    fn apply(&mut self, repo: &Path, file: &Path) -> Result<(), String> {
        repo_tab(self.tabs, repo)
            .ok_or_else(|| closed(file))?
            .apply_dropped_patch(file.to_path_buf(), self.ctx)
    }
}

impl PatchQueue {
    pub(super) fn push(&mut self, file: PathBuf, repo: PathBuf) {
        self.waiting.push_back((file, repo));
    }

    pub(super) fn is_empty(&self) -> bool {
        self.waiting.is_empty() && self.current.is_none()
    }

    /// Takes in how the patch being applied went and gives the next one to its tab, until one
    /// has to wait. Returns what to tell (each an error).
    #[cfg(test)]
    pub(super) fn advance(&mut self, tabs: &mut [Tab], ctx: &egui::Context) -> Vec<String> {
        self.advance_in(&mut tabs.iter_mut().collect::<Vec<_>>(), ctx)
    }

    fn advance_in(&mut self, tabs: &mut [&mut Tab], ctx: &egui::Context) -> Vec<String> {
        self.advance_with(&mut LiveTargets { tabs, ctx })
    }

    fn advance_with(&mut self, targets: &mut impl PatchTargets) -> Vec<String> {
        let mut said = Vec::new();
        loop {
            if let Some((file, repo)) = self.current.clone() {
                let Some(busy) = targets.busy(&repo) else {
                    self.current = None;
                    said.push(closed(&file));
                    continue;
                };
                match targets.outcome(&repo) {
                    None if busy => return said,
                    None | Some(Ok(())) => self.current = None,
                    Some(Err(e)) => {
                        self.current = None;
                        said.push(self.failed(&file, &repo, &e));
                    }
                }
            }
            let Some((file, repo)) = self.waiting.pop_front() else {
                return said;
            };
            let Some(busy) = targets.busy(&repo) else {
                said.push(closed(&file));
                continue;
            };
            if busy {
                self.waiting.push_front((file, repo));
                return said;
            }
            match targets.apply(&repo, &file) {
                Ok(()) => self.current = Some((file, repo)),
                Err(e) => said.push(crate::i18n::message(
                    "interaction-drop-not-applied",
                    &[
                        ("name", name_of(&file).into()),
                        ("error", crate::git::display_error(&e).into()),
                    ],
                )),
            }
        }
    }

    /// `file` failed in `repo`: the patches dropped on it after it are left out.
    fn failed(&mut self, file: &Path, repo: &Path, err: &str) -> String {
        let before = self.waiting.len();
        self.waiting.retain(|(_, r)| r != repo);
        let left = before - self.waiting.len();
        crate::i18n::message(
            if left > 0 {
                "interaction-drop-failed-following"
            } else {
                "interaction-drop-failed"
            },
            &[
                ("name", name_of(file).into()),
                ("error", crate::git::display_error(err).into()),
                ("count", left.into()),
            ],
        )
    }
}

/// What is said of a patch whose repository's tab was closed before it was applied.
fn closed(file: &Path) -> String {
    crate::i18n::message("interaction-drop-closed", &[("name", name_of(file).into())])
}

#[cfg(test)]
mod recording_tests {
    use super::*;

    struct Targets {
        repo: PathBuf,
        window: u8,
        busy: bool,
        outcomes: VecDeque<Result<(), String>>,
        applied: Vec<(PathBuf, PathBuf, u8)>,
        consumed: usize,
    }

    impl PatchTargets for Targets {
        fn busy(&mut self, repo: &Path) -> Option<bool> {
            (repo == self.repo).then_some(self.busy)
        }
        fn outcome(&mut self, _: &Path) -> Option<Result<(), String>> {
            let result = self.outcomes.pop_front();
            if result.is_some() {
                self.consumed += 1;
                self.busy = false;
            }
            result
        }
        fn apply(&mut self, repo: &Path, file: &Path) -> Result<(), String> {
            self.applied
                .push((repo.to_path_buf(), file.to_path_buf(), self.window));
            self.busy = true;
            Ok(())
        }
    }

    #[test]
    fn hidden_continuations_wait_advance_once_and_follow_the_live_session() {
        let repo = PathBuf::from("/recorded/session");
        let mut targets = Targets {
            repo: repo.clone(),
            window: 1,
            busy: true,
            outcomes: VecDeque::new(),
            applied: vec![],
            consumed: 0,
        };
        let mut queue = PatchQueue::default();
        queue.push("first.patch".into(), repo.clone());
        queue.push("second.patch".into(), repo.clone());
        assert!(queue.advance_with(&mut targets).is_empty());
        assert!(targets.applied.is_empty());
        targets.busy = false;
        assert!(queue.advance_with(&mut targets).is_empty());
        assert_eq!(targets.applied, [(repo.clone(), "first.patch".into(), 1)]);
        targets.window = 2;
        targets.outcomes.push_back(Ok(()));
        assert!(queue.advance_with(&mut targets).is_empty());
        assert_eq!(
            targets.applied,
            [
                (repo.clone(), "first.patch".into(), 1),
                (repo, "second.patch".into(), 2)
            ]
        );
        assert_eq!(targets.consumed, 1);
        assert!(queue.advance_with(&mut targets).is_empty());
        assert_eq!(targets.applied.len(), 2);
        targets.outcomes.push_back(Ok(()));
        assert!(queue.advance_with(&mut targets).is_empty());
        assert!(queue.is_empty());
        assert_eq!(targets.consumed, 2);
    }
}

impl App {
    /// Takes in what was dropped on the window and handles it: folders open, patches go to the
    /// repository tab they were dropped on. Shows a hint while files are dragged over it.
    pub(super) fn dropped_files(&mut self, ctx: &egui::Context) {
        let (dropped, hovered) =
            ctx.input(|i| (i.raw.dropped_files.clone(), !i.raw.hovered_files.is_empty()));
        // The tab dropped on, before a dropped folder opens another.
        let target = match self.tabs.get(self.active) {
            Some(Tab::Repo(r)) => Some(r.path.clone()),
            _ => None,
        };
        for file in dropped {
            let path = file.path().to_owned();
            match kind_of(&path) {
                DropKind::Folder => self.open_dropped_folder(&path, ctx),
                DropKind::Patch => match &target {
                    Some(repo) => self.patches.push(path, repo.clone()),
                    None => {
                        let msg = self.no_repo_for(&path);
                        self.report(msg);
                    }
                },
                DropKind::Other => self.report(AppError::unsupported(name_of(&path))),
            }
        }
        if hovered {
            let on_repo = target.is_some();
            drop_hint(ctx, on_repo);
        }
    }

    /// Continuations belong to model ticks, including passes with no visible UI.
    pub(super) fn advance_patches(&mut self, ctx: &egui::Context) {
        if !self.patches.is_empty() {
            let mut tabs = self
                .tabs
                .iter_mut()
                .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
                .collect::<Vec<_>>();
            for msg in self.patches.advance_in(&mut tabs, ctx) {
                self.toasts.push(Toast::new(msg, true));
            }
            if !self.patches.is_empty() {
                // Its tab may be busy with something that doesn't wake the window when done.
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
    }

    /// Why a patch dropped where no repository is shown wasn't applied.
    fn no_repo_for(&self, patch: &Path) -> AppError {
        AppError::patch_target(
            self.tabs.iter().any(|t| matches!(t, Tab::Repo(_))),
            &name_of(patch),
        )
    }

    /// Opens the dropped folder's repository, or says why it can't.
    fn open_dropped_folder(&mut self, path: &Path, ctx: &egui::Context) {
        // One in WSL is reached in the background, as when opened otherwise.
        if crate::remote::is_remote(path) {
            self.open_repo(path, ctx);
            return;
        }
        match repo_root(path) {
            Ok(root) => self.open_root(root, ctx),
            Err(e) => self.report(retained_folder_error(path, &e)),
        }
    }
}

#[cfg(test)]
fn patch_target_message(has_repo: bool, name: &str) -> String {
    AppError::patch_target(has_repo, name).delivery_text()
}

/// A veil over the window while files are dragged over it, saying what a drop does there.
fn drop_hint(ctx: &egui::Context, on_repo: bool) {
    let t = Tokens::get(ctx);
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("drop-hint")));
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(110));
    let text = if on_repo {
        crate::i18n::text("interaction-drop-hint-repo")
    } else {
        crate::i18n::text("interaction-drop-hint-folder")
    };
    let galley = painter.layout_no_wrap(text.to_owned(), theme::semibold(16.0), t.text);
    let pill = Align2::CENTER_CENTER
        .anchor_size(rect.center(), galley.size())
        .expand2(egui::vec2(22.0, 14.0));
    painter.rect(
        pill,
        t.radius_lg,
        t.card,
        Stroke::new(1.0, t.field_border),
        egui::StrokeKind::Inside,
    );
    painter.galley(pill.shrink2(egui::vec2(22.0, 14.0)).min, galley, t.text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_items_are_told_apart() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(classify(&p("/x/repo"), true), DropKind::Folder);
        // A folder named like a patch is still a folder.
        assert_eq!(classify(&p("/x/fix.patch"), true), DropKind::Folder);
        assert_eq!(classify(&p("/x/fix.patch"), false), DropKind::Patch);
        assert_eq!(classify(&p("/x/FIX.DIFF"), false), DropKind::Patch);
        assert_eq!(classify(&p("/x/mail.Eml"), false), DropKind::Patch);
        assert_eq!(classify(&p("/x/all.mbox"), false), DropKind::Patch);
        assert_eq!(
            classify(&p("/home/me/my patches/0001-Größe ändern.patch"), false),
            DropKind::Patch
        );
        assert_eq!(classify(&p("/x/notes.txt"), false), DropKind::Other);
        assert_eq!(classify(&p("/x/Makefile"), false), DropKind::Other);
        assert_eq!(classify(&p("/x/.patch"), false), DropKind::Other);
        assert_eq!(classify(&p("/x/fix.patch.txt"), false), DropKind::Other);
        assert_eq!(classify(&p("/x/日本語.diff"), false), DropKind::Patch);
    }

    #[test]
    fn a_folder_outside_a_repository_says_so() {
        let msg = folder_error(
            Path::new("/home/me/My Stuff"),
            "fatal: not a git repository (or any of the parent directories): .git",
        );
        assert_eq!(
            msg,
            "My Stuff isn't in a git repository: drop a repository's folder to open it"
        );
        assert_eq!(
            folder_error(Path::new("/x/y"), "detected dubious ownership"),
            "Could not open y: detected dubious ownership"
        );
    }

    /// One on another machine is told by its name, without asking that machine.
    #[test]
    fn remote_drops_are_told_by_their_name() {
        assert_eq!(
            kind_of(Path::new("ssh://box/home/me/fix.PATCH")),
            DropKind::Patch
        );
        assert_eq!(
            kind_of(Path::new("ssh://box/home/me/repo")),
            DropKind::Folder
        );
        #[cfg(windows)]
        assert_eq!(
            kind_of(Path::new(r"\\wsl.localhost\Ubuntu\home\me\repo")),
            DropKind::Folder
        );
    }

    /// The message a dropped folder outside any repository gets, through what opening it
    /// really says.
    #[test]
    fn a_folder_outside_a_repository_says_so_through_repo_root() {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-drop plain Ü {}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let err = repo_root(&dir).err().unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        let name = name_of(&dir);
        assert_eq!(
            folder_error(&dir, &err),
            format!("{name} isn't in a git repository: drop a repository's folder to open it"),
            "{err}"
        );
    }

    fn repo(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-dropq-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
        ] {
            assert!(crate::git::run(Some(&dir), args).is_ok());
        }
        let _ = std::fs::write(dir.join("f.txt"), "one\n");
        assert!(crate::git::run(Some(&dir), &["add", "-A"]).is_ok());
        assert!(crate::git::run(Some(&dir), &["commit", "-q", "-m", "one"]).is_ok());
        dir
    }

    /// Runs the queue until it is empty (or it took too long).
    fn drain(queue: &mut PatchQueue, tabs: &mut [Tab], ctx: &egui::Context) -> Vec<String> {
        let mut said = Vec::new();
        for _ in 0..400 {
            for tab in tabs.iter_mut() {
                if let Tab::Repo(r) = tab {
                    r.poll(ctx);
                }
            }
            said.extend(queue.advance(tabs, ctx));
            if queue.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        said
    }

    /// Patches go to the tab they were dropped on, whichever tab is shown meanwhile; one that
    /// fails leaves out the ones dropped after it on the same tab, by its own outcome (not the
    /// tab's status line); a closed tab's patch says so.
    #[test]
    fn dropped_patches_go_to_their_own_tab_and_stop_after_a_failure() {
        let ctx = egui::Context::default();
        let a = repo("a");
        let b = repo("b");
        let patches = std::env::temp_dir().join(format!("gitcraft-dropq-p-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&patches);
        let good = patches.join("good.patch");
        let _ = std::fs::write(
            &good,
            "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1 +1,2 @@\n one\n+two\n",
        );
        let bad = patches.join("bad.diff");
        let _ = std::fs::write(
            &bad,
            "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-nothing\n+else\n",
        );
        let mut tabs = vec![
            Tab::Repo(Box::new(RepoTab::new_deferred(a.clone(), &ctx))),
            Tab::Repo(Box::new(RepoTab::new_deferred(b.clone(), &ctx))),
        ];
        let mut queue = PatchQueue::default();
        // Dropped on A: good, bad, then good again (left out after bad fails).
        queue.push(good.clone(), a.clone());
        queue.push(bad.clone(), a.clone());
        queue.push(good.clone(), a.clone());
        let said = drain(&mut queue, &mut tabs, &ctx);
        // Git on Windows may check files out with CRLF (core.autocrlf): compare the lines.
        let read = |p: PathBuf| {
            std::fs::read_to_string(p)
                .unwrap_or_default()
                .replace("\r\n", "\n")
        };
        let in_a = read(a.join("f.txt"));
        let in_b = read(b.join("f.txt"));
        // Dropped on B, whose tab is then closed.
        queue.push(good, b.clone());
        tabs.pop();
        let closed_said = drain(&mut queue, &mut tabs, &ctx);
        for dir in [&a, &b, &patches] {
            let _ = std::fs::remove_dir_all(dir);
        }
        assert_eq!(in_a, "one\ntwo\n");
        assert_eq!(in_b, "one\n");
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].starts_with("bad.diff failed:"), "{said:?}");
        assert!(said[0].ends_with("(so the 1 patch dropped after it was not applied)"));
        assert_eq!(
            closed_said,
            vec!["good.patch wasn't applied: its repository's tab was closed".to_owned()]
        );
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;

    struct Recorded {
        repo: PathBuf,
        raw_error: String,
        applied: Vec<PathBuf>,
    }

    impl PatchTargets for Recorded {
        fn busy(&mut self, repo: &Path) -> Option<bool> {
            (repo == self.repo).then_some(false)
        }
        fn outcome(&mut self, _: &Path) -> Option<Result<(), String>> {
            Some(Err(self.raw_error.clone()))
        }
        fn apply(&mut self, _: &Path, file: &Path) -> Result<(), String> {
            self.applied.push(file.to_owned());
            Err(self.raw_error.clone())
        }
    }

    #[test]
    fn actual_recorded_queue_keeps_raw_outcomes_and_paths_while_translating_only_captions() {
        crate::i18n::set_language("de");
        let repo = PathBuf::from("/opaque/repo");
        let file = PathBuf::from("/opaque/日本語 failed: literal.patch");
        let raw = "external diagnostic 日本語\n: untouched".to_owned();
        let mut targets = Recorded {
            repo: repo.clone(),
            raw_error: raw.clone(),
            applied: vec![],
        };
        let mut queue = PatchQueue {
            current: Some((file.clone(), repo.clone())),
            waiting: VecDeque::new(),
        };
        queue.push("one.patch".into(), repo.clone());
        queue.push("two.patch".into(), repo.clone());
        let gone = PathBuf::from("/opaque/closed");
        queue.push("closed.patch".into(), gone);
        let said = queue.advance_with(&mut targets);
        assert_eq!(said.len(), 2);
        assert_eq!(
            said[0],
            format!(
                "日本語 failed: literal.patch ist fehlgeschlagen: {raw} (deshalb wurden die 2 danach abgelegten Patches nicht angewendet)"
            )
        );
        assert_eq!(
            said[1],
            "closed.patch wurde nicht angewendet: Der Tab des zugehörigen Repositorys wurde geschlossen"
        );
        assert!(queue.is_empty());
        assert!(targets.applied.is_empty());
        assert_eq!(targets.raw_error, raw);
        // A direct apply rejection keeps the original file request and external diagnostic.
        queue.push(file.clone(), repo.clone());
        assert_eq!(
            queue.advance_with(&mut targets),
            [format!(
                "日本語 failed: literal.patch wurde nicht angewendet: {raw}"
            )]
        );
        assert_eq!(targets.applied, [file.clone()]);
        assert!(queue.is_empty());
        // Singular and zero variants use the same original retain decision.
        queue.push("later.patch".into(), repo.clone());
        assert!(
            queue
                .failed(&file, &repo, &raw)
                .ends_with("(deshalb wurde der danach abgelegte Patch nicht angewendet)")
        );
        assert_eq!(
            queue.failed(&file, &repo, &raw),
            format!("日本語 failed: literal.patch ist fehlgeschlagen: {raw}")
        );
        crate::i18n::set_language("en-US");
        queue.push("later.patch".into(), repo.clone());
        assert_eq!(
            queue.failed(&file, &repo, &raw),
            format!(
                "日本語 failed: literal.patch failed: {raw} (so the 1 patch dropped after it was not applied)"
            )
        );
        assert_eq!(
            closed(&file),
            "日本語 failed: literal.patch wasn't applied: its repository's tab was closed"
        );
    }

    #[test]
    fn actual_folder_classifier_and_target_guidance_preserve_opaque_names_and_unknown_errors() {
        crate::i18n::set_language("de");
        let path = Path::new("/opaque/日本語 isn't in a git repository");
        let raw = "FATAL: NOT A GIT REPOSITORY";
        assert_eq!(
            folder_error(path, raw),
            "日本語 isn't in a git repository liegt nicht in einem Git-Repository: Ziehe einen Repository-Ordner hierher, um ihn zu öffnen"
        );
        assert_eq!(raw, "FATAL: NOT A GIT REPOSITORY");
        let unknown = "external 日本語\n: diagnostic";
        assert_eq!(
            folder_error(path, unknown),
            format!("日本語 isn't in a git repository konnte nicht geöffnet werden: {unknown}")
        );
        assert_eq!(
            patch_target_message(true, "literal.patch"),
            "Zeige den Tab des zugehörigen Repositorys und lege literal.patch dort ab, um den Patch anzuwenden"
        );
        assert!(
            patch_target_message(false, "日本語.patch").starts_with(
                "Um 日本語.patch anzuwenden, muss zuerst ein Repository geöffnet sein"
            )
        );
        crate::i18n::set_language("en-US");
        assert_eq!(
            patch_target_message(true, "literal.patch"),
            "To apply literal.patch, show its repository's tab, then drop the patch there"
        );
        assert_eq!(
            folder_error(Path::new("/x/y"), unknown),
            format!("Could not open y: {unknown}")
        );
    }
}
