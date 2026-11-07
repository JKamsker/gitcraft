//! Back and forward: what each state of the big area is as a place, the keys and buttons that
//! step, and stepping through a real repository's views.

use std::path::PathBuf;

use egui::{Event, Key, Modifiers, PointerButton, RawInput};

use super::super::nav_history::NavHistory;
use super::{View, nav_step, record};
use crate::git;
use crate::repo::file_view::{FileTab, FileView};
use crate::repo::{Action, DiffKey, DiffView, RepoTab, WIP_HASH};
use crate::settings::Settings;

/// A tab on no repository, for what its state maps to.
fn bare_tab() -> RepoTab {
    let ctx = egui::Context::default();
    RepoTab::new_deferred("/nonexistent/nav-test".into(), &ctx)
}

fn commit_key(hash: &str, path: &str) -> DiffKey {
    DiffKey::Commit {
        hash: hash.to_owned(),
        path: path.to_owned(),
    }
}

fn work_file(path: &str, area: git::Area) -> git::StatusFile {
    git::StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: 'M',
        area,
    }
}

/// A diff's place, with the commit selected meanwhile.
fn diff_at(key: DiffKey, selected: Option<&str>) -> View {
    View::Diff {
        key,
        selected: selected.map(str::to_owned),
    }
}

fn graph(selected: Option<&str>) -> View {
    View::Graph {
        selected: selected.map(str::to_owned),
    }
}

/// The diff shows below the graph from now on, for this context.
fn below_the_graph(ctx: &egui::Context, on: bool) {
    let mut settings = Settings::get(ctx);
    settings.diff_beside_graph = on;
    settings.put_for_test(ctx);
}

#[test]
fn nothing_open_is_the_graph_with_its_selection() {
    let tab = bare_tab();
    assert_eq!(tab.current_view(), Some(graph(None)));
}

#[test]
fn an_open_diff_is_the_diff() {
    let mut tab = bare_tab();
    let key = commit_key("abc", "dir with space/ünï.txt");
    tab.diff = Some(DiffView::new(key.clone()));
    assert_eq!(tab.current_view(), Some(diff_at(key, None)));
    let work = DiffKey::Work(work_file("a b.rs", git::Area::Unstaged));
    tab.diff = Some(DiffView::new(work.clone()));
    assert_eq!(tab.current_view(), Some(diff_at(work, None)));
}

#[test]
fn a_file_view_is_its_path_rev_and_tab() {
    let mut tab = bare_tab();
    tab.file_view = Some(FileView::new(
        "söme/file.txt",
        Some("abc".into()),
        FileTab::Blame,
    ));
    assert_eq!(
        tab.current_view(),
        Some(View::File {
            path: "söme/file.txt".into(),
            rev: Some("abc".into()),
            tab: FileTab::Blame,
        })
    );
    // A diff over it (not beside its history) is what shows.
    let key = commit_key("def", "söme/file.txt");
    tab.diff = Some(DiffView::new(key.clone()));
    assert_eq!(tab.current_view(), Some(diff_at(key, None)));
}

#[test]
fn editors_are_not_places() {
    let mut tab = bare_tab();
    // A conflicted file whose editor is still being read.
    tab.diff = Some(DiffView::new(DiffKey::Work(work_file(
        "c.txt",
        git::Area::Conflicted,
    ))));
    assert_eq!(tab.current_view(), None);
}

#[test]
fn builds_and_pull_requests_are_places() {
    let ctx = egui::Context::default();
    let mut tab = bare_tab();
    tab.open_pr(7, &ctx);
    assert_eq!(tab.current_view(), Some(View::Pr(7)));
    // A file of the pull request belongs to it.
    tab.diff = Some(DiffView::new(DiffKey::Range {
        from: "a".into(),
        to: Some("b".into()),
        path: "x.rs".into(),
    }));
    assert_eq!(tab.current_view(), Some(View::Pr(7)));
    tab.diff = None;
    tab.open_ci("0123456789abcdef".into(), None, &ctx);
    assert_eq!(
        tab.current_view(),
        Some(View::Ci("0123456789abcdef".into()))
    );
}

#[test]
fn selecting_in_the_graph_refines_its_place() {
    let mut h = NavHistory::with_capacity(10);
    record(&mut h, graph(None), false);
    record(&mut h, graph(Some("a")), false);
    record(&mut h, graph(Some("b")), false);
    assert_eq!(h.entries(), &[graph(Some("b"))]);
    let diff = diff_at(commit_key("b", "f"), Some("b"));
    record(&mut h, diff.clone(), false);
    record(&mut h, diff.clone(), false);
    // Commits selected above the same diff (below the graph) refine its place.
    record(&mut h, diff_at(commit_key("b", "f"), Some("c")), false);
    let diff = diff_at(commit_key("b", "f"), Some("c"));
    record(&mut h, graph(Some("c")), false);
    assert_eq!(
        h.entries(),
        &[graph(Some("b")), diff.clone(), graph(Some("c"))]
    );
    // Back on the first graph place, selecting keeps the diff forward.
    h.go(-1);
    h.go(-1);
    record(&mut h, graph(Some("d")), false);
    assert_eq!(h.current(), Some(&graph(Some("d"))));
    assert_eq!(h.peek(1), Some(&diff));
}

#[test]
fn following_a_link_in_the_graph_is_a_step() {
    let mut h = NavHistory::with_capacity(10);
    record(&mut h, graph(Some("a")), false);
    record(&mut h, graph(Some("b")), true);
    record(&mut h, graph(Some("c")), true);
    // The same commit again is no step.
    record(&mut h, graph(Some("c")), true);
    assert_eq!(
        h.entries(),
        &[graph(Some("a")), graph(Some("b")), graph(Some("c"))]
    );
    // A click after the link refines the link's place.
    record(&mut h, graph(Some("d")), false);
    assert_eq!(h.current(), Some(&graph(Some("d"))));
    assert_eq!(h.entries().len(), 3);
}

#[test]
fn labels_say_where_a_step_goes() {
    let label = |v: View| v.label(false);
    assert_eq!(label(graph(None)), "the graph");
    assert_eq!(
        label(graph(Some("fc02e181f5672558"))),
        "the graph at fc02e181"
    );
    assert_eq!(label(graph(Some(WIP_HASH))), "the graph at the changes");
    assert_eq!(graph(Some("abc")).label(true), "the graph without the diff");
    assert_eq!(
        label(diff_at(commit_key("fc02e181f567", "dir/ünï cödé.rs"), None)),
        "ünï cödé.rs in fc02e181"
    );
    let work = |area| diff_at(DiffKey::Work(work_file("src/mod.rs", area)), None);
    assert_eq!(
        label(work(git::Area::Unstaged)),
        "the unstaged changes of mod.rs"
    );
    assert_eq!(
        label(work(git::Area::Staged)),
        "the staged changes of mod.rs"
    );
    assert_eq!(label(work(git::Area::Conflicted)), "the conflict in mod.rs");
    assert_eq!(label(work(git::Area::Untracked)), "the new file mod.rs");
    let range = |to: Option<&str>| {
        diff_at(
            DiffKey::Range {
                from: "aaaaaaaaaa".into(),
                to: to.map(str::to_owned),
                path: "a b.txt".into(),
            },
            None,
        )
    };
    assert_eq!(
        label(range(Some("bbbbbbbbbb"))),
        "a b.txt from aaaaaaaa to bbbbbbbb"
    );
    assert_eq!(
        label(range(None)),
        "a b.txt from aaaaaaaa to the working tree"
    );
    let file = |rev: Option<&str>, tab| View::File {
        path: "docs/README.md".into(),
        rev: rev.map(str::to_owned),
        tab,
    };
    assert_eq!(
        label(file(None, FileTab::History)),
        "the history of README.md"
    );
    assert_eq!(
        label(file(Some("0123456789"), FileTab::Blame)),
        "the blame of README.md at 01234567"
    );
    assert_eq!(label(View::Pr(3)), "pull request #3");
    assert_eq!(
        label(View::Ci("0123456789".into())),
        "the builds of 01234567"
    );
}

/// What `nav_step` makes of one frame's input.
fn step_for(events: Vec<Event>, modifiers: Modifiers, keys: bool) -> Option<i32> {
    let ctx = egui::Context::default();
    let mut step = None;
    let mut all = vec![Event::ModifiersChanged(modifiers)];
    all.extend(events);
    let input = RawInput {
        events: all,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        step = ui.ctx().input(|i| nav_step(i, keys));
    });
    output.textures_delta.clear();
    step
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn mouse(button: PointerButton) -> Vec<Event> {
    let pos = egui::pos2(10.0, 10.0);
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    ]
}

#[test]
fn alt_arrows_and_mouse_buttons_step() {
    let alt = Modifiers::ALT;
    assert_eq!(
        step_for(vec![key(Key::ArrowLeft, alt)], alt, true),
        Some(-1)
    );
    assert_eq!(
        step_for(vec![key(Key::ArrowRight, alt)], alt, true),
        Some(1)
    );
    assert_eq!(
        step_for(mouse(PointerButton::Extra1), Modifiers::NONE, true),
        Some(-1)
    );
    assert_eq!(
        step_for(mouse(PointerButton::Extra2), Modifiers::NONE, true),
        Some(1)
    );
}

#[test]
fn other_keys_and_text_fields_do_not_step() {
    let alt = Modifiers::ALT;
    // Alt+↑ / Alt+↓ go from change to change in a diff.
    assert_eq!(step_for(vec![key(Key::ArrowUp, alt)], alt, true), None);
    assert_eq!(step_for(vec![key(Key::ArrowDown, alt)], alt, true), None);
    // Plain arrows move in the graph.
    let none = Modifiers::NONE;
    assert_eq!(step_for(vec![key(Key::ArrowLeft, none)], none, true), None);
    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
    assert_eq!(
        step_for(vec![key(Key::ArrowLeft, alt_shift)], alt_shift, true),
        None
    );
    let alt_cmd = Modifiers::ALT | Modifiers::COMMAND;
    assert_eq!(
        step_for(vec![key(Key::ArrowLeft, alt_cmd)], alt_cmd, true),
        None
    );
    // In a text field Alt+← jumps a word; the mouse's buttons still go back.
    assert_eq!(step_for(vec![key(Key::ArrowLeft, alt)], alt, false), None);
    assert_eq!(
        step_for(mouse(PointerButton::Extra1), none, false),
        Some(-1)
    );
    assert_eq!(step_for(mouse(PointerButton::Primary), none, true), None);
}

/// A repository with two commits; the second adds a file in a folder whose names have spaces
/// and accents. Gives the folder and the file's path.
fn repo(name: &str) -> (PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("gitcraft-nav-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = "dir with space/ünï cödé.txt";
    assert!(std::fs::create_dir_all(dir.join("dir with space")).is_ok());
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "core.quotepath", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    assert!(std::fs::write(dir.join(file), "one\ntwo\n").is_ok());
    assert!(git::run(Some(&dir), &["add", "-A"]).is_ok());
    assert!(git::run(Some(&dir), &["commit", "-q", "-m", "add the file"]).is_ok());
    (dir, file.to_owned())
}

/// Polls the tab until `done` holds (or a few seconds passed).
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

/// Applies an action as a frame of the tab does, noting the view it leaves.
fn act(tab: &mut RepoTab, action: Action, ctx: &egui::Context) {
    tab.apply(action, ctx);
    tab.record_view();
}

/// Clicks the row of commit `hash` in the graph.
fn select(tab: &mut RepoTab, hash: &str, ctx: &egui::Context) {
    let row = tab.index.get(hash).copied();
    assert!(row.is_some(), "{hash} is not in the graph");
    act(tab, Action::Select(row.unwrap_or_default()), ctx);
}

/// A tab on `dir` with its history read.
fn opened(dir: &std::path::Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new(dir.to_path_buf(), ctx);
    wait(&mut tab, ctx, |t| !t.loading && t.commits.len() >= 2);
    assert!(tab.commits.len() >= 2, "history not read");
    tab.record_view();
    tab
}

#[test]
fn back_and_forward_through_a_repository() {
    let (dir, file) = repo("walk");
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    let head = git::head(&dir);
    let row = tab.index[&head];

    act(&mut tab, Action::Select(row), &ctx);
    let key = commit_key(&head, &file);
    act(&mut tab, Action::OpenDiff(key.clone()), &ctx);
    act(
        &mut tab,
        Action::FileView {
            path: file.clone(),
            rev: Some(head.clone()),
            tab: FileTab::History,
        },
        &ctx,
    );
    assert!(tab.diff.is_none());
    assert!(tab.file_view.is_some());

    // Back: the diff, then the graph with the commit selected.
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert!(tab.file_view.is_none());
    assert_eq!(tab.diff.as_ref().map(|d| d.key.clone()), Some(key.clone()));
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert!(tab.diff.is_none());
    assert_eq!(tab.selected_hash(), Some(head.clone()));
    assert_eq!(
        tab.current_view(),
        Some(View::Graph {
            selected: Some(head.clone())
        })
    );
    // Nothing further back.
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(
        tab.current_view(),
        Some(View::Graph {
            selected: Some(head.clone())
        })
    );

    // Forward again: the diff, then the file's history.
    act(&mut tab, Action::Navigate(1), &ctx);
    assert_eq!(tab.diff.as_ref().map(|d| d.key.clone()), Some(key.clone()));
    act(&mut tab, Action::Navigate(1), &ctx);
    let shown = tab.file_view.as_ref().map(|fv| (fv.path.as_str(), fv.tab));
    assert_eq!(shown, Some((file.as_str(), FileTab::History)));

    // Opening something new after going back drops what was forward.
    act(&mut tab, Action::Navigate(-1), &ctx);
    act(&mut tab, Action::CloseDiff, &ctx);
    assert!(!tab.nav.can_forward());
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(tab.diff.as_ref().map(|d| d.key.clone()), Some(key));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_diff_beside_its_file_history_is_the_history() {
    let (dir, file) = repo("beside");
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    let head = git::head(&dir);
    let key = commit_key(&head, &file);
    act(&mut tab, Action::OpenDiff(key.clone()), &ctx);
    act(
        &mut tab,
        Action::FileView {
            path: file.clone(),
            rev: None,
            tab: FileTab::History,
        },
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        t.file_view.as_ref().is_some_and(|v| v.history.is_some())
    });
    act(&mut tab, Action::OpenDiff(key.clone()), &ctx);
    assert!(tab.history_beside_diff());
    let history = View::File {
        path: file,
        rev: None,
        tab: FileTab::History,
    };
    assert_eq!(tab.current_view(), Some(history.clone()));
    assert_eq!(tab.nav.current(), Some(&history));

    // Back to the same diff: it stays as it is, not read again.
    let request = tab.diff.as_ref().map(|d| d.request);
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert!(tab.file_view.is_none());
    assert_eq!(tab.diff.as_ref().map(|d| d.key.clone()), Some(key));
    assert_eq!(tab.diff.as_ref().map(|d| d.request), request);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The commits of `repo`: (HEAD, its parent).
fn head_and_base(dir: &std::path::Path) -> (String, String) {
    let base = git::run(Some(dir), &["rev-parse", "HEAD~1"]).unwrap_or_default();
    (git::head(dir), base.trim().to_owned())
}

#[test]
fn back_to_a_diff_selects_its_commit_again() {
    let (dir, file) = repo("reselect");
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    let (head, base) = head_and_base(&dir);
    let key = commit_key(&head, &file);
    act(&mut tab, Action::OpenDiff(key.clone()), &ctx);
    act(&mut tab, Action::CloseDiff, &ctx);
    select(&mut tab, &base, &ctx);
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(tab.diff.as_ref().map(|d| d.key.clone()), Some(key.clone()));
    assert_eq!(tab.selected_hash(), Some(head.clone()));

    // Below the graph, the commits selected while the diff shows are its place's.
    below_the_graph(&ctx, true);
    select(&mut tab, &base, &ctx);
    assert_eq!(tab.nav.current(), Some(&diff_at(key.clone(), Some(&base))));
    act(&mut tab, Action::CloseDiff, &ctx);
    select(&mut tab, &head, &ctx);
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(tab.diff.as_ref().map(|d| d.key.clone()), Some(key));
    assert_eq!(tab.selected_hash(), Some(base));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn following_links_to_commits_are_steps() {
    let (dir, _) = repo("links");
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    let (head, base) = head_and_base(&dir);
    act(&mut tab, Action::SelectHash(base.clone()), &ctx);
    act(&mut tab, Action::SelectHash(head.clone()), &ctx);
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(tab.selected_hash(), Some(base));
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(tab.selected_hash(), Some(head.clone()));
    // A click in the graph is no step of its own: on the link's place, it changes that one
    // (and places that end up the same are one).
    act(&mut tab, Action::Navigate(1), &ctx);
    select(&mut tab, &head, &ctx);
    assert_eq!(tab.nav.entries(), &[graph(Some(&head))]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gone_places_are_passed_over_and_said_so() {
    let (dir, file) = repo("vanished");
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    let head = git::head(&dir);
    assert_eq!(tab.nav.current(), Some(&graph(Some(&head))));

    // A change of the working tree that was committed since, between two equal places: both
    // become one, and there is nothing further back.
    let work = |area| DiffKey::Work(work_file(&file, area));
    tab.nav.visit(diff_at(work(git::Area::Unstaged), None));
    tab.nav.visit(graph(Some(&head)));
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert!(tab.diff.is_none());
    assert_eq!(
        tab.status,
        Some((format!("{file} has no unstaged changes any more"), false))
    );
    assert_eq!(tab.nav.entries(), &[graph(Some(&head))]);

    // Past it to the place before, with the area's words.
    let history = View::File {
        path: file.clone(),
        rev: None,
        tab: FileTab::History,
    };
    tab.nav.visit(history.clone());
    tab.nav.visit(diff_at(work(git::Area::Staged), None));
    tab.nav.visit(graph(Some(&head)));
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(tab.nav.current(), Some(&history));
    assert!(tab.file_view.is_some());
    assert_eq!(
        tab.status,
        Some((format!("{file} has no staged changes any more"), false))
    );
    // Forward goes straight back to the graph.
    assert_eq!(tab.nav.peek(1), Some(&graph(Some(&head))));

    // A commit that is not in the graph (any more): the graph, and a note.
    tab.status = None;
    let gone = "0123456789abcdef0123456789abcdef01234567".to_owned();
    tab.nav.visit(graph(Some(&gone)));
    tab.nav
        .visit(diff_at(commit_key(&head, &file), Some(&head)));
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(
        tab.status,
        Some((
            "Commit 01234567 is no longer in the graph".to_owned(),
            false
        ))
    );
    assert_eq!(
        tab.nav.current(),
        Some(&graph(tab.selected_hash().as_deref()))
    );
    assert_ne!(tab.selected_hash(), Some(gone));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_change_shows_as_it_is_now() {
    let (dir, file) = repo("stale");
    let ctx = egui::Context::default();
    assert!(std::fs::write(dir.join(&file), "one\nthree\n").is_ok());
    let mut tab = opened(&dir, &ctx);
    wait(&mut tab, &ctx, |t| !t.work.unstaged.is_empty());
    let now = tab.work.unstaged.first().cloned();
    assert_eq!(now.as_ref().map(|f| f.path.as_str()), Some(file.as_str()));
    // Noted with another status letter (renamed, say) than it has now.
    let mut then = work_file(&file, git::Area::Unstaged);
    then.code = 'R';
    then.old_path = Some("old name.txt".into());
    tab.nav.visit(diff_at(DiffKey::Work(then), Some(WIP_HASH)));
    tab.nav.visit(graph(None));
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(
        tab.diff.as_ref().map(|d| d.key.clone()),
        now.map(DiffKey::Work)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn going_back_keeps_a_commit_message_being_edited() {
    let mut tab = bare_tab();
    let ctx = egui::Context::default();
    tab.nav.visit(graph(None));
    tab.nav.visit(View::Pr(3));
    tab.reword = Some(crate::repo::Reword {
        hash: "abc".into(),
        subject: "half written".into(),
        body: String::new(),
        focus: false,
        fill: false,
    });
    tab.navigate(-1, &ctx);
    assert_eq!(tab.nav.current(), Some(&View::Pr(3)));
    assert!(tab.reword.is_some());
    let message = tab.status.clone().map(|(m, _)| m).unwrap_or_default();
    assert!(message.contains("commit message"), "{message}");
}

#[test]
fn below_the_graph_the_diff_stays_while_selecting() {
    let (dir, file) = repo("split");
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    let (head, base) = head_and_base(&dir);
    let key = commit_key(&head, &file);

    // In place of the graph: selecting another commit closes the diff.
    act(&mut tab, Action::OpenDiff(key.clone()), &ctx);
    assert!(!tab.diff_beside_graph(&ctx));
    let (base_row, head_row) = (tab.index[&base], tab.index[&head]);
    act(&mut tab, Action::Select(base_row), &ctx);
    assert!(tab.diff.is_none());

    // Below the graph: it stays, and follows the diff opened next.
    below_the_graph(&ctx, true);
    act(&mut tab, Action::OpenDiff(key.clone()), &ctx);
    assert!(tab.diff_beside_graph(&ctx));
    act(&mut tab, Action::Select(head_row), &ctx);
    assert_eq!(tab.diff.as_ref().map(|d| d.key.clone()), Some(key));
    let other = commit_key(&base, &file);
    act(&mut tab, Action::OpenDiff(other.clone()), &ctx);
    assert_eq!(tab.diff.as_ref().map(|d| d.key.clone()), Some(other));
    // A file's history keeps its own place.
    act(
        &mut tab,
        Action::FileView {
            path: file.clone(),
            rev: None,
            tab: FileTab::History,
        },
        &ctx,
    );
    assert!(!tab.diff_can_split());

    // A pull request's file, or one over the builds, goes with its page.
    let range = DiffKey::Range {
        from: base.clone(),
        to: Some(head.clone()),
        path: file,
    };
    for page in ["pr", "ci"] {
        if page == "pr" {
            tab.open_pr(7, &ctx);
        } else {
            tab.open_ci(head.clone(), None, &ctx);
        }
        tab.diff = Some(DiffView::new(range.clone()));
        act(&mut tab, Action::Select(base_row), &ctx);
        assert!(tab.diff.is_none(), "the diff of the {page} page stayed");
        assert!(tab.pr_view.is_none() && tab.ci_view.is_none());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Frames as the window draws them, with time for the background work to come back.
fn frames(harness: &mut egui_kittest::Harness<'_, RepoTab>) {
    for _ in 0..20 {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Drawn as the window draws it: Alt+↓ moves in the graph alone; with the diff below the
/// graph, Alt+← goes back to the graph, Alt+→ forward to the diff again, Esc closes it.
#[test]
fn the_keys_step_through_the_drawn_tab() {
    let (dir, file) = repo("drawn");
    let ctx = egui::Context::default();
    let tab = opened(&dir, &ctx);
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_ui_state(
            |ui, tab: &mut RepoTab| {
                // The app's fonts are there from the frame after they are given.
                if ui.ctx().cumulative_frame_nr() == 0 {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                } else {
                    tab.ui(ui);
                }
            },
            tab,
        );
    below_the_graph(&harness.ctx, true);
    frames(&mut harness);
    let (head, base) = head_and_base(&dir);
    assert_eq!(harness.state().selected_hash(), Some(head.clone()));
    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    frames(&mut harness);
    assert_eq!(harness.state().selected_hash(), Some(base));
    harness.key_press(Key::ArrowUp);
    frames(&mut harness);

    let key = commit_key(&head, &file);
    harness.state_mut().queue(Action::OpenDiff(key.clone()));
    frames(&mut harness);
    assert!(harness.state().diff_beside_graph(&harness.ctx));
    // Alt+↓ is the diff's now: the graph's selection stays.
    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    frames(&mut harness);
    assert_eq!(harness.state().selected_hash(), Some(head));

    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowLeft);
    frames(&mut harness);
    assert!(harness.state().diff.is_none());
    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowRight);
    frames(&mut harness);
    let shown = harness.state().diff.as_ref().map(|d| d.key.clone());
    assert_eq!(shown, Some(key));
    harness.key_press(Key::Escape);
    frames(&mut harness);
    assert!(harness.state().diff.is_none());
    // Esc went to the graph: back is the diff again.
    assert!(matches!(
        harness.state().nav.peek(-1),
        Some(View::Diff { .. })
    ));
    let _ = std::fs::remove_dir_all(&dir);
}
