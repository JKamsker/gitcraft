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

/// Sets up a graph place without the first-file reveal of a user row/link action.
fn select_graph(tab: &mut RepoTab, hash: &str, ctx: &egui::Context) {
    assert!(tab.diff.is_none());
    tab.restore_commit_selection(tab.index[hash], ctx);
    tab.record_view();
    assert_eq!(tab.current_view(), Some(graph(Some(hash))));
    assert!(!tab.commit_changes.follows_history());
}

/// Equal graph neighbours merge, while a distinct forward diff remains reachable.
fn assert_graph_refinement(
    tab: &mut RepoTab,
    hash: &str,
    forward: Option<View>,
    ctx: &egui::Context,
) {
    let current = graph(Some(hash));
    if let Some(next) = &forward {
        assert_eq!(tab.nav.entries(), &[current.clone(), next.clone()]);
    } else {
        assert_eq!(tab.nav.entries(), &[current.clone()]);
    }
    assert_eq!(tab.current_view(), Some(current.clone()));
    assert_eq!(tab.nav.current(), Some(&current));
    assert!(!tab.nav.can_back());
    assert_eq!(tab.nav.can_forward(), forward.is_some());
    assert_eq!(tab.nav.peek(1), forward.as_ref());
    if let Some(next) = forward {
        act(tab, Action::Navigate(1), ctx);
        assert_eq!(tab.current_view(), Some(next));
        act(tab, Action::Navigate(-1), ctx);
        assert_eq!(tab.current_view(), Some(current));
    }
    assert!(tab.diff.is_none());
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
    select_graph(&mut tab, &head, &ctx);
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
    select_graph(&mut tab, &head, &ctx);
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(tab.diff.as_ref().map(|d| d.key.clone()), Some(key));
    assert_eq!(tab.selected_hash(), Some(base));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn following_links_to_commits_are_steps() {
    let (dir, file) = repo("links");
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    let (head, base) = head_and_base(&dir);
    act(&mut tab, Action::SelectHash(base.clone()), &ctx);
    act(&mut tab, Action::SelectHash(head.clone()), &ctx);
    // A ready detail reveals the first file; a pending one still shows the graph.
    let forward = tab.diff.as_ref().map(|view| {
        assert_eq!(view.key, commit_key(&head, &file));
        diff_at(view.key.clone(), Some(&head))
    });
    assert_eq!(
        tab.current_view(),
        Some(forward.clone().unwrap_or_else(|| graph(Some(&head))))
    );
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(tab.selected_hash(), Some(base));
    act(&mut tab, Action::Navigate(-1), &ctx);
    assert_eq!(tab.selected_hash(), Some(head.clone()));
    // Refining a graph place merges equal neighbours. A user row selection instead
    // reveals the first changed file, which can make a distinct diff place.
    act(&mut tab, Action::Navigate(1), &ctx);
    select_graph(&mut tab, &head, &ctx);
    assert_graph_refinement(&mut tab, &head, forward, &ctx);
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
        tab.status.as_deref().cloned(),
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
        tab.status.as_deref().cloned(),
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
        tab.status.as_deref().cloned(),
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
    let message = tab
        .status
        .as_deref()
        .cloned()
        .map(|(m, _)| m)
        .unwrap_or_default();
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
