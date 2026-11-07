//! The workspaces' cards against real repositories, through the start page's own state.
use super::*;
use crate::across::workspaces::Workspaces;

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args)
}

fn scratch(name: &str) -> Result<PathBuf, String> {
    let dir =
        std::env::temp_dir().join(format!("gitcraft-workspaces-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// A clone of a bare repository, set up to commit.
fn clone_of(origin: &Path, dest: &Path) -> Result<(), String> {
    let parent = dest.parent().ok_or("no parent")?;
    let name = dest.file_name().ok_or("no name")?.to_string_lossy();
    git(parent, &["clone", "-q", &origin.to_string_lossy(), &name])?;
    for args in [
        &["config", "user.name", "T"][..],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(dest, args)?;
    }
    Ok(())
}

/// Polls `view` until nothing runs any more.
fn settle(view: &mut WorkspacesView, ws: &Workspaces, ctx: &egui::Context) -> Result<(), String> {
    let known: HashSet<PathBuf> = ws.all_repos().into_iter().collect();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
    view.poll(&known, ctx);
    while view.busy() {
        if std::time::Instant::now() > until {
            return Err("the looks never finished".to_owned());
        }
        view.poll(&known, ctx);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Ok(())
}

/// What the start page does with a workspace: looks at each repository (one gone), then
/// fetches them all and looks again, which then shows what the remote got meanwhile.
#[test]
fn a_workspace_is_looked_at_and_fetched() -> Result<(), String> {
    let root = scratch("fetch")?;
    git(&root, &["init", "-q", "--bare", "-b", "main", "origin.git"])?;
    let origin = root.join("origin.git");
    let mine = root.join("mine ü");
    let theirs = root.join("theirs");
    clone_of(&origin, &mine)?;
    clone_of(&origin, &theirs)?;
    std::fs::write(theirs.join("a"), "1").map_err(|e| e.to_string())?;
    git(&theirs, &["add", "-A"])?;
    git(&theirs, &["commit", "-qm", "one"])?;
    git(&theirs, &["push", "-q", "origin", "HEAD:main"])?;
    git(&mine, &["fetch", "-q"])?;
    git(&mine, &["checkout", "-q", "main"])?;
    std::fs::write(theirs.join("a"), "2").map_err(|e| e.to_string())?;
    git(&theirs, &["commit", "-qam", "two"])?;
    git(&theirs, &["push", "-q", "origin", "HEAD:main"])?;
    std::fs::write(mine.join("new.txt"), "x").map_err(|e| e.to_string())?;
    let gone = root.join("gone");

    let mut ws = Workspaces::default();
    let i = ws.create("W")?;
    ws.add_repo(i, &mine);
    ws.add_repo(i, &gone);
    let ctx = egui::Context::default();
    let mut view = WorkspacesView::default();
    view.look_unasked(&ws.all_repos());
    settle(&mut view, &ws, &ctx)?;
    let seen = |view: &WorkspacesView, p: &Path| view.repos.get(p).and_then(|s| s.seen.clone());
    let o = seen(&view, &mine).ok_or("not looked at")??;
    assert_eq!(o.branch.as_deref(), Some("main"));
    assert_eq!((o.ahead, o.behind, o.changed), (0, 0, 1));
    assert!(o.last_fetch.is_some());
    assert!(seen(&view, &gone).is_some_and(|r| r.is_err()));
    // Looked at already: not again until asked.
    view.look_unasked(&ws.all_repos());
    assert!(view.looks.is_idle());

    view.fetch(ws.all_repos());
    settle(&mut view, &ws, &ctx)?;
    let o = seen(&view, &mine).ok_or("not looked at")??;
    assert_eq!(o.behind, 1, "fetched, then looked at again");
    assert_eq!(
        view.repos
            .get(&mine)
            .and_then(|s| s.fetch_error.clone())
            .map(|e| e.0),
        None
    );
    assert!(
        view.repos
            .get(&gone)
            .is_some_and(|s| s.fetch_error.is_some())
    );
    Ok(())
}

/// A repository being looked at isn't looked at a second time meanwhile.
#[test]
fn a_busy_repository_isnt_asked_twice() {
    let mut view = WorkspacesView::default();
    let p = PathBuf::from("/nowhere/x");
    assert_eq!(
        view.mark(vec![p.clone(), p.clone()], Busy::Looking),
        vec![p.clone()]
    );
    assert!(view.mark(vec![p.clone()], Busy::Fetching).is_empty());
}

/// A repository taken out of every workspace is forgotten, results arriving for it or not.
#[test]
fn a_removed_repository_is_forgotten() {
    let ctx = egui::Context::default();
    let mut view = WorkspacesView::default();
    view.look(vec![
        PathBuf::from("/nowhere/a"),
        PathBuf::from("/nowhere/b"),
    ]);
    let known: HashSet<PathBuf> = [PathBuf::from("/nowhere/b")].into_iter().collect();
    view.poll(&known, &ctx);
    assert!(!view.repos.contains_key(Path::new("/nowhere/a")));
    assert!(view.repos.contains_key(Path::new("/nowhere/b")));
}

/// One on another machine with no connection open isn't looked at unasked (which could ask
/// for a password); asked, it is.
#[test]
fn another_machine_is_only_reached_when_asked() {
    let mut view = WorkspacesView::default();
    let far = PathBuf::from("ssh://gitcraft-test-never-connected/x");
    view.look_unasked(std::slice::from_ref(&far));
    assert!(view.looks.is_idle());
    view.look(vec![far.clone()]);
    assert_eq!(view.looks.waiting, vec![far]);
}

#[test]
fn long_branch_names_are_cut_in_the_middle() {
    assert_eq!(cut_middle("feature/JIRA-1234-long", 9), "feat…long");
    assert_eq!(cut_middle("short", 9), "short");
    assert_eq!(cut_middle("ünïcödé-bränch", 5), "ün…ch");
    assert_eq!(cut_middle("abc", 0), "a…");
}

#[test]
fn the_tooltip_says_why_not_click_to_open() {
    let failed = RepoState {
        seen: Some(Err("fatal: cannot change to '/x'".into())),
        ..RepoState::default()
    };
    let tip = row_tooltip(Path::new("/x"), Some(&failed));
    assert!(
        tip.contains("Couldn't look at it: fatal: cannot change"),
        "{tip}"
    );
    assert!(!tip.contains("Click to open"));
    assert!(row_tooltip(Path::new("/x"), None).contains("Click to open"));
}

#[test]
fn texts_of_a_row() {
    assert_eq!(changed_text(3), "3 changed");
    assert_eq!(fetched_text(None, 100), "never fetched");
    assert_eq!(fetched_text(Some(100 - 7200), 100), "fetched 2 hours ago");
    assert_eq!(repo_name(Path::new("/home/me/wörk repo")), "wörk repo");
    assert_eq!(repo_name(Path::new("ssh://build/~/x")), "x");
}

#[test]
fn a_rows_parts_say_what_matters() {
    let t = Tokens::get(&egui::Context::default());
    let o = Overview {
        branch: Some("main".into()),
        changed: 2,
        ahead: 1,
        behind: 0,
        has_remote: true,
        ..Overview::default()
    };
    let texts: Vec<String> = overview_parts(&o, 0, &t)
        .into_iter()
        .map(|p| p.text)
        .collect();
    assert_eq!(texts, ["main", "2 changed", "1", "never fetched"]);
    let detached = Overview {
        head: "abc1234".into(),
        ..Overview::default()
    };
    let texts: Vec<String> = overview_parts(&detached, 0, &t)
        .into_iter()
        .map(|p| p.text)
        .collect();
    assert_eq!(texts, ["detached at abc1234", "no remote"]);
}

#[test]
fn accessibility_workspace_remove_is_reachable_without_hover() {
    use egui_kittest::{Harness, kittest::Queryable};
    let path = PathBuf::from("/workspace/日本語");
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, removed: &mut bool| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            let t = Tokens::get(ui.ctx());
            if matches!(workspace_row(ui, &path, None, &t), Some(RowClick::Remove)) {
                *removed = true;
            }
            crate::a11y::end_frame(ui.ctx());
        },
        false,
    );
    h.run();
    h.key_press(egui::Key::Tab);
    h.run();
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(
        h.get_by_label("Remove 日本語 from the workspace")
            .is_focused()
    );
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(*h.state());
}
