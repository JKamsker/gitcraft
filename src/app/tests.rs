//! Saved state, the active tab on start, the window icon and palette matching.

use eframe::Storage as _;

use super::palette::fuzzy;
use super::{Saved, active_among_kept, window_icon};
use crate::tab_look::{TabColor, TabLook};

/// A storage that keeps strings in memory, as eframe's own does on disk.
#[derive(Default)]
struct Memory(std::collections::HashMap<String, String>);

impl eframe::Storage for Memory {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

#[test]
fn tab_looks_survive_a_restart() {
    let look = TabLook {
        icon: Some("rocket".to_owned()),
        color: TabColor::Purple,
    };
    let mut saved = Saved::default();
    saved.tab_looks.insert("C:/code/app".into(), look.clone());
    let mut storage = Memory::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &saved);
    let back: Option<Saved> = eframe::get_value(&storage, eframe::APP_KEY);
    assert!(
        back.is_some_and(|b| b.tab_looks.get(std::path::Path::new("C:/code/app")) == Some(&look))
    );
}

/// A tab whose folder is gone doesn't make the one after the active tab active.
#[test]
fn active_tab_survives_a_gone_folder() {
    assert_eq!(active_among_kept(&[true, true, true], 2), 2);
    assert_eq!(active_among_kept(&[false, true, true], 2), 1);
    assert_eq!(active_among_kept(&[true, false, true], 2), 1);
    assert_eq!(active_among_kept(&[true, true, false], 1), 1);
    assert_eq!(active_among_kept(&[true, false, true], 1), 0);
    assert_eq!(active_among_kept(&[false, true], 0), 0);
    assert_eq!(active_among_kept(&[], 3), 0);
    assert_eq!(active_among_kept(&[true, true], usize::MAX), 1);
}

#[test]
fn window_icon_decodes() {
    let icon = window_icon();
    assert_eq!((icon.width, icon.height), (256, 256));
    assert!(
        icon.rgba.chunks(4).any(|p| p[3] > 0),
        "the icon isn't blank"
    );
}

#[test]
fn fuzzy_matching() {
    assert!(fuzzy("check out main", "cmain"));
    assert!(fuzzy("check out main", "co main"));
    assert!(!fuzzy("fetch", "push"));
    assert!(fuzzy("anything", ""));
}

/// A repository in WSL (`GITCRAFT_TEST_WSL=<distro>`; skipped without): the window is driven
/// through actions of every kind, and none of them makes the window's thread wait for WSL.
#[test]
fn a_remote_repository_never_holds_up_the_window() -> Result<(), String> {
    use crate::remote::{self, Host, Request};
    use std::time::{Duration, Instant};
    type H<'a> = egui_kittest::Harness<'a, super::App>;

    let Ok(distro) = std::env::var("GITCRAFT_TEST_WSL") else {
        return Ok(());
    };
    let host = Host::Wsl(distro);
    let dir = format!("/tmp/gitcraft-window-{}", std::process::id());
    let setup = format!(
        concat!(
            "set -e; rm -rf {d}; mkdir -p {d}; cd {d}; git init -q -b main\n",
            "git config user.name t; git config user.email t@t\n",
            "printf '1\\n2\\n3\\n' > a.txt; git add a.txt; git commit -q -m one\n",
            "printf '1\\n2\\n3\\n4\\n' > a.txt; git commit -q -a -m two\n",
            "git tag -a v1 -m 'the first'\n",
            "printf '1\\nX\\n3\\n4\\n' > a.txt; echo x > ignored.txt\n",
        ),
        d = dir
    );
    let made = remote::exec(
        &host,
        &Request {
            script: setup,
            ..Request::default()
        },
    )?;
    assert_eq!(made.code, 0, "{}", String::from_utf8_lossy(&made.stderr));
    let refused_before = remote::refused_on_ui_thread();

    let path = remote::encode(&host, &dir);
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1400.0, 860.0])
        .build_eframe(|cc| super::App::new(cc, Some(path)));
    // Frames as the window draws them, with time for the background work to come back.
    let run_for = |harness: &mut H<'_>, ms: u64| {
        let until = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < until {
            harness.step();
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let settled = |harness: &H<'_>| {
        let state = harness.state();
        matches!(state.tabs.get(state.active),
            Some(super::Tab::Repo(r)) if !r.opening() && r.settled())
    };
    let with_tab = |harness: &mut H<'_>, f: &dyn Fn(&mut crate::repo::RepoTab)| {
        let state = harness.state_mut();
        let active = state.active;
        if let Some(super::Tab::Repo(r)) = state.tabs.get_mut(active) {
            f(r);
        }
    };
    // Connected and read.
    for _ in 0..300 {
        run_for(&mut harness, 100);
        if settled(&harness) {
            break;
        }
    }
    assert!(settled(&harness), "the repository never opened");
    for phase in [1, 2] {
        with_tab(&mut harness, &|r| {
            for action in r.sample_actions(phase) {
                r.queue(action);
            }
        });
        run_for(&mut harness, 4000);
        // A dialog an action opened is let go, so that the next ones run.
        with_tab(&mut harness, &|r| r.close_dialog());
    }
    let refused = remote::refused_on_ui_thread() - refused_before;
    // Cleaned up from another thread: this one is the window's now.
    let cleanup = Request {
        script: format!("rm -rf {dir}"),
        ..Request::default()
    };
    let _ = std::thread::spawn(move || remote::exec(&host, &cleanup)).join();
    assert_eq!(refused, 0, "commands asked of WSL from the window's thread");
    Ok(())
}

/// The tabs of the profiles not in use survive a restart, and saves from before profiles load.
#[test]
fn profile_tabs_survive_a_restart() {
    let mut saved = Saved::default();
    saved.profile_states.insert(
        2,
        super::profiles::ProfileState {
            open: vec!["C:/code/work".into()],
            active: 0,
            recent: vec!["C:/code/work".into()],
            theme: crate::theme::ThemeId::new("Light"),
        },
    );
    let mut storage = Memory::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &saved);
    let back: Option<Saved> = eframe::get_value(&storage, eframe::APP_KEY);
    assert_eq!(
        back.map(|b| b.profile_states),
        Some(saved.profile_states.clone())
    );

    let mut old = Memory::default();
    old.set_string(eframe::APP_KEY, "(open: [\"C:/a\"], active: 0)".to_owned());
    let back: Option<Saved> = eframe::get_value(&old, eframe::APP_KEY);
    let back = back.unwrap_or_default();
    assert_eq!(back.open, [std::path::PathBuf::from("C:/a")]);
    assert!(back.profile_states.is_empty());
}

/// The theme was an enum before there were more themes, saved bare (`theme: Light`): such a
/// save still loads, and a theme of the new kind saves and loads back.
#[test]
fn themes_saved_before_and_now_load() {
    let mut old = Memory::default();
    old.set_string(
        eframe::APP_KEY,
        "(open: [], theme: MediumGray, profile_states: { 2: (theme: Light) })".to_owned(),
    );
    let back: Option<Saved> = eframe::get_value(&old, eframe::APP_KEY);
    let back = back.unwrap_or_default();
    assert_eq!(back.theme, crate::theme::ThemeId::new("MediumGray"));
    assert_eq!(
        back.profile_states.get(&2).map(|s| s.theme.clone()),
        Some(crate::theme::ThemeId::new("Light"))
    );
    let saved = Saved {
        theme: crate::theme::ThemeId::file("my theme.json"),
        ..Saved::default()
    };
    let mut storage = Memory::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &saved);
    let back: Option<Saved> = eframe::get_value(&storage, eframe::APP_KEY);
    assert_eq!(back.map(|b| b.theme), Some(saved.theme));
}

/// What a later start hands over opens as the command line does: a subfolder opens its
/// repository's tab, handed over again it switches to that tab, and "come to the front" leaves
/// the tabs as they are.
#[test]
fn a_folder_handed_over_opens_its_repositorys_tab_once() -> Result<(), String> {
    use crate::instance::Request;

    let root = std::env::temp_dir().join(format!(
        "gitcraft-handoff-{}-{}",
        std::process::id(),
        crate::remote::askpass::random_token()
    ));
    let sub = root.join("src");
    std::fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status()
        .map_err(|e| e.to_string())?;
    assert!(init.success());
    let expected = super::repo_root(&root)?;

    let mut harness = egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_eframe(|cc| super::App::new(cc, None));
    harness.step();
    let ctx = harness.ctx.clone();
    let repo_tabs = |app: &super::App| -> Vec<std::path::PathBuf> {
        app.tabs
            .iter()
            .filter_map(|t| match t {
                super::Tab::Repo(r) => Some(r.path.clone()),
                _ => None,
            })
            .collect()
    };

    harness
        .state_mut()
        .handle_handoff(Request::Open(sub.clone()), &ctx);
    harness.step();
    assert_eq!(repo_tabs(harness.state()), vec![expected.clone()]);

    harness.state_mut().new_tab();
    harness
        .state_mut()
        .handle_handoff(Request::Open(root.clone()), &ctx);
    harness.state_mut().handle_handoff(Request::Focus, &ctx);
    harness.step();
    let state = harness.state();
    assert_eq!(repo_tabs(state), vec![expected.clone()]);
    assert!(
        matches!(state.tabs.get(state.active), Some(super::Tab::Repo(r)) if r.path == expected)
    );
    drop(harness);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

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
