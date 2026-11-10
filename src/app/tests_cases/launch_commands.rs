/// `gitcraft log <file>` handed over by a later start, from a subfolder: the window opens the
/// file's repository and shows that file's history; a refused command line (a revision that
/// looks like an option) opens nothing.
#[test]
fn a_command_line_handed_over_opens_the_file_s_history() -> Result<(), String> {
    use crate::instance::Request;
    use std::time::{Duration, Instant};

    let root = std::env::temp_dir().join(format!(
        "gitcraft-handoff-log ü {}-{}",
        std::process::id(),
        crate::remote::askpass::random_token()
    ));
    let sub = root.join("src dir");
    std::fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
    std::fs::write(sub.join("a b.txt"), "x\n").map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["add", "-A"],
        &["commit", "-q", "-m", "one"],
    ] {
        crate::git::run(Some(&root), args)?;
    }
    let expected = super::repo_root(&root)?;
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_eframe(|cc| super::App::new(cc, None));
    harness.step();
    let ctx = harness.ctx.clone();
    harness.state_mut().handle_handoff(
        Request::Launch {
            cwd: sub.clone(),
            args: vec!["show".into(), "--output=/tmp/x".into()],
        },
        &ctx,
    );
    harness.state_mut().handle_handoff(
        Request::Launch {
            cwd: sub.clone(),
            args: vec!["log".into(), "a b.txt".into()],
        },
        &ctx,
    );
    let until = Instant::now() + Duration::from_secs(20);
    let shown = loop {
        harness.step();
        let state = harness.state();
        let history = match state.tabs.get(state.active) {
            Some(super::Tab::Repo(r)) if r.path == expected => r.file_view_path(),
            _ => None,
        };
        if history.is_some() || Instant::now() > until {
            break history;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let tabs = harness
        .state()
        .tabs
        .iter()
        .filter(|t| matches!(t, super::Tab::Repo(_)))
        .count();
    drop(harness);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(shown.as_deref(), Some("src dir/a b.txt"));
    assert_eq!(tabs, 1);
    Ok(())
}

/// Several command lines handed over in the same moment (History in gitcraft on files of two
/// repositories) are all carried out: each repository gets its tab.
#[test]
fn command_lines_handed_over_together_are_all_carried_out() -> Result<(), String> {
    use crate::instance::Request;
    use std::time::{Duration, Instant};

    let base = std::env::temp_dir().join(format!(
        "gitcraft-handoff-many-{}-{}",
        std::process::id(),
        crate::remote::askpass::random_token()
    ));
    let mut roots = Vec::new();
    for name in ["one", "two"] {
        let dir = base.join(name);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("f.txt"), "x\n").map_err(|e| e.to_string())?;
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["add", "-A"],
            &["commit", "-q", "-m", "one"],
        ] {
            crate::git::run(Some(&dir), args)?;
        }
        roots.push(super::repo_root(&dir)?);
    }
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_eframe(|cc| super::App::new(cc, None));
    harness.step();
    let ctx = harness.ctx.clone();
    for root in &roots {
        harness.state_mut().handle_handoff(
            Request::Launch {
                cwd: root.clone(),
                args: vec!["log".into(), "f.txt".into()],
            },
            &ctx,
        );
    }
    let open = |app: &super::App| -> Vec<std::path::PathBuf> {
        app.tabs
            .iter()
            .filter_map(|t| match t {
                super::Tab::Repo(r) => Some(r.path.clone()),
                _ => None,
            })
            .collect()
    };
    let until = Instant::now() + Duration::from_secs(20);
    while open(harness.state()).len() < 2 && Instant::now() < until {
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut tabs = open(harness.state());
    drop(harness);
    let _ = std::fs::remove_dir_all(&base);
    tabs.sort();
    roots.sort();
    assert_eq!(tabs, roots);
    Ok(())
}

/// A `gitcraft://` link from the browser, handed over: the commit it points at is selected in
/// the open clone of its repository (found by its remote, written another way); a link to a
/// repository with no clone here opens the clone dialog with its URL.
#[test]
fn a_link_from_the_browser_opens_in_its_clone_or_offers_to_clone() -> Result<(), String> {
    use crate::instance::Request;
    use std::time::{Duration, Instant};

    let root = std::env::temp_dir().join(format!(
        "gitcraft-link-handoff ü {}-{}",
        std::process::id(),
        crate::remote::askpass::random_token()
    ));
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["remote", "add", "origin", "git@github.com:Owner/Repo.git"],
        &["commit", "-q", "--allow-empty", "-m", "one"],
        &["commit", "-q", "--allow-empty", "-m", "two"],
    ] {
        crate::git::run(Some(&root), args)?;
    }
    let first = crate::git::run(Some(&root), &["rev-parse", "HEAD~1"])?
        .trim()
        .to_owned();
    let expected = super::repo_root(&root)?;
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_eframe(|cc| super::App::new(cc, None));
    harness.step();
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .handle_handoff(Request::Open(root.clone()), &ctx);
    let link = format!(
        "gitcraft://open?url=https%3A%2F%2Fgithub.com%2Fowner%2Frepo%2Fcommit%2F{}",
        first.get(..12).unwrap_or(&first)
    );
    harness.state_mut().handle_handoff(
        Request::Launch {
            cwd: std::env::temp_dir(),
            args: vec![link],
        },
        &ctx,
    );
    let until = Instant::now() + Duration::from_secs(20);
    let selected = loop {
        harness.step();
        let state = harness.state();
        let selected = match state.tabs.get(state.active) {
            Some(super::Tab::Repo(r)) if r.path == expected => r.selected_commit(),
            _ => None,
        };
        if selected.as_deref() == Some(first.as_str()) || Instant::now() > until {
            break selected;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    harness.state_mut().handle_handoff(
        Request::Launch {
            cwd: std::env::temp_dir(),
            args: vec!["https://codeberg.org/someone/else/pulls/3".into()],
        },
        &ctx,
    );
    let until = Instant::now() + Duration::from_secs(20);
    let offered = loop {
        harness.step();
        let url = harness.state().clone_url();
        if url.is_some() || Instant::now() > until {
            break url;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    drop(harness);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(selected, Some(first));
    assert_eq!(
        offered.as_deref(),
        Some("https://codeberg.org/someone/else.git")
    );
    Ok(())
}

#[test]
fn session_can_be_owned_by_a_deferred_viewport() {
    fn send<T: Send>() {}
    send::<crate::repo::RepoTab>();
    send::<super::App>();
}
