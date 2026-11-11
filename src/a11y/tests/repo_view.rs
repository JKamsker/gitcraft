//! The repository view of a real repository, driven headlessly by the keyboard: Tab reaches the
//! graph, its rows carry their names, ↑ / ↓ take the keyboard from row to row.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::git;
use crate::repo::RepoTab;
use crate::settings::Settings;

/// A repository with a spaced, unicode branch name and messages, in a fresh folder.
fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft a11y {name} {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("ä file.txt");
    let run = |args: &[&str]| git::run(Some(&dir), args).map(|_| ());
    run(&["init", "-q", "-b", "main"])?;
    run(&["config", "user.name", "Jöns Ñ"])?;
    run(&["config", "user.email", "j@example.com"])?;
    run(&["config", "commit.gpgSign", "false"])?;
    for (n, message) in ["First ✓ commit", "Second\n\nwith a body", "Third: 日本語"]
        .iter()
        .enumerate()
    {
        std::fs::write(&file, format!("{n}\n")).map_err(|e| e.to_string())?;
        run(&["add", "-A"])?;
        run(&["commit", "-q", "-m", message])?;
    }
    run(&["branch", "feature/äpfel", "HEAD~1"])?;
    run(&["tag", "v1", "HEAD~2"])?;
    Ok(dir)
}

struct Repository(RepoTab);

impl eframe::App for Repository {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
        super::super::end_frame(ui.ctx());
    }
}

type H<'a> = Harness<'a, Repository>;

/// eframe's creation callback runs before the first pass: set_fonts only takes effect
/// at the beginning of a pass, even when a repository read is immediately ready.
fn prepared_harness<S: eframe::App + 'static>(
    create: impl FnOnce(&egui::Context) -> S,
) -> Harness<'static, S> {
    Harness::builder()
        .with_size([1400.0, 860.0])
        .build_eframe(move |cc| {
            let ctx = &cc.egui_ctx;
            // Nothing asked of the network: no avatars, no background fetch.
            let settings = Settings {
                avatars: false,
                auto_fetch_minutes: 0,
                ..Settings::default()
            };
            settings.set(ctx);
            crate::theme::install_fonts(ctx);
            crate::theme::apply(ctx, &crate::theme::ThemeId::default(), &Default::default());
            create(ctx)
        })
}

fn harness(path: &Path) -> H<'static> {
    let path = path.to_owned();
    prepared_harness(move |ctx| Repository(RepoTab::new(path, ctx)))
}

/// Frames until the repository is read (the git work runs in the background).
fn settle(harness: &mut H<'_>) {
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        harness.step();
        let tab = &harness.state().0;
        if !tab.opening() && tab.settled() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    for _ in 0..5 {
        harness.step();
    }
}

#[test]
fn first_frame_fonts_support_ready_and_pending_repository_content() -> Result<(), String> {
    struct Content {
        ready: bool,
        first_pass: Option<(bool, u64)>,
    }
    impl Content {
        fn paint(&mut self, ui: &mut egui::Ui) {
            self.first_pass
                .get_or_insert((self.ready, ui.ctx().cumulative_pass_nr()));
            if self.ready {
                ui.label(egui::RichText::new("Ready heading").font(crate::theme::semibold(16.0)));
                ui.label(egui::RichText::new("Ready row").font(crate::theme::medium(13.0)));
            } else {
                ui.label("Pending repository read");
            }
        }
    }
    impl eframe::App for Content {
        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            self.paint(ui);
        }
    }
    for prepared in [false, true] {
        for ready in [false, true] {
            let content = Content {
                ready,
                first_pass: None,
            };
            let mut harness = if prepared {
                prepared_harness(move |_| content)
            } else {
                Harness::builder().build_ui_state(
                    |ui, content: &mut Content| {
                        if crate::theme::test_paint::fonts_before_paint(ui.ctx()) {
                            content.paint(ui);
                        }
                    },
                    content,
                )
            };
            let (first_ready, pass) = harness.state().first_pass.ok_or("first content paint")?;
            assert_eq!(first_ready, ready);
            if prepared {
                assert_eq!(pass, 0, "prepared fonts allow painting on the first pass");
            } else {
                assert!(pass > 0, "callback font installation must finish first");
            }
            if !ready {
                harness.get_by_label("Pending repository read");
                assert!(harness.query_by_label("Ready heading").is_none());
                harness.state_mut().ready = true;
                harness.step();
            }
            harness.get_by_label("Ready heading");
            harness.get_by_label("Ready row");
        }
    }
    Ok(())
}

/// What a screen reader says of the widget with the keyboard: its name, else its text (a label
/// that can be clicked) or a text field's hint. egui's own panel splitters, which have neither, are
/// left out (`None` as when nothing has the keyboard).
fn focused(harness: &H<'_>) -> Option<String> {
    let node = harness.query_by(|n| n.is_focused())?;
    let node = node.accesskit_node();
    if node.role() == egui::accesskit::Role::Unknown && node.label().is_none() {
        return None;
    }
    let said = node
        .label()
        .or_else(|| node.value().filter(|v| !v.is_empty()))
        .or_else(|| node.placeholder().map(str::to_owned));
    Some(said.unwrap_or_default())
}

fn press(harness: &mut H<'_>, key: Key) {
    harness.key_press(key);
    for _ in 0..3 {
        harness.step();
    }
}

#[test]
fn the_graph_is_reached_with_tab_and_read_aloud() -> Result<(), String> {
    let dir = repo("graph")?;
    let mut harness = harness(&dir);
    settle(&mut harness);
    let head = git::run(Some(&dir), &["rev-parse", "HEAD"])?;
    // As the graph shortens it.
    let head = crate::repo::short(head.trim());

    // Every row carries its name: hash, summary, author, age and branches.
    let wanted = format!("commit {head}");
    let row = harness.get_by_label_contains(&wanted);
    let label = row.accesskit_node().label().unwrap_or_default();
    assert!(
        label.starts_with(&format!("commit {head}, Third: 日本語, Jöns Ñ, ")),
        "{label}"
    );
    assert!(label.ends_with("branch main, checked out"), "{label}");
    assert!(harness.query_by_label_contains("Second, Jöns Ñ").is_some());
    assert!(
        harness
            .query_by_label_contains("branch feature/äpfel")
            .is_some()
    );
    assert!(harness.query_by_label_contains("tag v1").is_some());

    // Tab moves the keyboard through the view until it reaches the graph; each stop is named.
    let mut stops = Vec::new();
    let mut reached = None;
    for _ in 0..120 {
        press(&mut harness, Key::Tab);
        let Some(now) = focused(&harness) else {
            continue;
        };
        if now.starts_with("commit ") {
            reached = Some(now);
            break;
        }
        stops.push(now);
    }
    let reached = reached.ok_or_else(|| format!("Tab never reached the graph: {stops:?}"))?;
    assert!(!stops.is_empty(), "the toolbar and sidebar come first");
    let unnamed: Vec<_> = stops.iter().filter(|s| s.trim().is_empty()).collect();
    assert!(unnamed.is_empty(), "stops without a name among {stops:?}");
    assert!(reached.starts_with(&format!("commit {head}")), "{reached}");

    // ↓ takes the selection and the keyboard to the next commit, ↑ back.
    press(&mut harness, Key::ArrowDown);
    let second = focused(&harness).unwrap_or_default();
    assert!(second.contains("Second, Jöns Ñ"), "{second}");
    assert!(second.contains("branch feature/äpfel"), "{second}");
    press(&mut harness, Key::ArrowUp);
    assert!(
        focused(&harness)
            .unwrap_or_default()
            .starts_with(&format!("commit {head}"))
    );

    // The graph is one stop: the next Tab leaves it.
    press(&mut harness, Key::Tab);
    let after = focused(&harness).unwrap_or_default();
    assert!(
        !after.starts_with("commit "),
        "Tab stayed in the graph: {after}"
    );

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
