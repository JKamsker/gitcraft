//! Find in a real repository's diff, file and blame, through the code the window runs: the
//! diff and the file read as the views read them, the keys sent to a whole tab.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, RawInput};

use super::{Find, Query, Seen, search_diff};
use crate::git::{self, DiffOptions};
use crate::repo::file_view::FileTab;
use crate::repo::{Action, DiffKey, RepoTab, diff};

/// A file in a folder with a space, its name not ASCII.
const PATH: &str = "my dir/ünï cöde.txt";

/// A repository whose last commit changes [`PATH`] so that, side by side, an added line with a
/// match is on the row above a removed one with a match.
fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-find-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("my dir")).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| git::run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(args)?;
    }
    let write = |text: &str| std::fs::write(dir.join(PATH), text).map_err(|e| e.to_string());
    write("Grüße aus Köln\nplain a\nneedle b\nomega\n")?;
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "one"])?;
    write("GRÜSSE aus Köln\nNeedle A2\nplain B2\nomega\n")?;
    git(&["commit", "-qam", "two"])?;
    Ok(dir)
}

fn head(dir: &Path) -> Result<String, String> {
    Ok(git::run(Some(dir), &["rev-parse", "HEAD"])?
        .trim()
        .to_owned())
}

fn query(text: &str) -> Query {
    Query {
        text: text.to_owned(),
        ..Query::default()
    }
}

/// The diff of the last commit, read as the diff view reads it.
fn last_diff(dir: &Path) -> Result<diff::DiffData, String> {
    let key = DiffKey::Commit {
        hash: head(dir)?,
        path: PATH.to_owned(),
    };
    diff::load(dir, &key, DiffOptions::default(), true)
}

/// The matches of `find` in the order Enter steps through them, as (line, first character),
/// from the first.
fn stepped(find: &mut Find) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for _ in 0..find.count() {
        if let Some(hit) = find.current() {
            out.push((hit.line, hit.start));
        }
        find.step(1);
    }
    let first = find.order.first().map(|h| (h.line, h.start));
    if let Some(at) = out.iter().position(|h| Some(*h) == first) {
        out.rotate_left(at);
    }
    out
}

#[test]
fn a_diff_is_stepped_through_in_the_order_of_its_rows() -> Result<(), String> {
    let dir = repo("order")?;
    let data = last_diff(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    let data = data?;
    let ctx = egui::Context::default();
    let text: Vec<&str> = data.lines.iter().map(|l| l.text.as_str()).collect();
    let line_of = |t: &str| text.iter().position(|l| *l == t).unwrap_or(usize::MAX);
    let (removed, added) = (line_of("needle b"), line_of("Needle A2"));

    let mut find = Find::default();
    find.show();
    find.query = query("needle");
    let _ = search_diff(&mut find, &ctx, 1, &data, false);
    // Unified: the removed line comes first.
    assert_eq!(stepped(&mut find), [(removed, 0), (added, 0)]);
    // Side by side: the added line is on the row above, on the right. The current match
    // stays the one it was.
    let before = find.current().map(|h| h.line);
    let _ = search_diff(&mut find, &ctx, 1, &data, true);
    assert_eq!(find.current().map(|h| h.line), before);
    assert_eq!(stepped(&mut find), [(added, 0), (removed, 0)]);

    // Case folds beyond ASCII ("ß" matches "SS" only as a full fold, which isn't done).
    find.query = query("grüße");
    let _ = search_diff(&mut find, &ctx, 1, &data, false);
    assert_eq!(find.count(), 1);
    find.query.case_sensitive = true;
    find.query.text = "Needle".to_owned();
    let _ = search_diff(&mut find, &ctx, 1, &data, false);
    assert_eq!(stepped(&mut find), [(added, 0)]);
    // Hunk headers aren't searched.
    find.query = query("@@");
    let _ = search_diff(&mut find, &ctx, 1, &data, false);
    assert_eq!(
        (find.count(), find.summary()),
        (0, Some(("No results".into(), false)))
    );
    // A broken regular expression says so.
    find.query = query("needle(");
    find.query.regex = true;
    let _ = search_diff(&mut find, &ctx, 1, &data, false);
    assert!(
        find.summary()
            .is_some_and(|(text, error)| error && text.contains("regular"))
    );
    Ok(())
}

#[test]
fn stepping_goes_round_and_starts_where_the_reader_is() {
    let ctx = egui::Context::default();
    let lines: Vec<String> = (0..100).map(|i| format!("x{i}")).collect();
    let mut find = Find::default();
    find.show();
    find.query = query("x");
    // Scrolled to row 40 of 19-point rows.
    find.saw(Seen {
        y: 40.0 * 19.0,
        height: 190.0,
        text_w: 500.0,
        line_h: 19.0,
        x: 0.0,
    });
    let key = || super::SourceKey {
        what: super::Source::File {
            path: "a".into(),
            rev: None,
        },
        at: 1,
        len: 100,
    };
    find.update(
        &ctx,
        key(),
        100,
        |i| super::nth(&lines, i),
        super::Layout::Lines,
    );
    assert_eq!(find.current().map(|h| h.line), Some(40));
    // Already in view: no scrolling down; sideways neither.
    let hit = find.take_reveal();
    assert!(hit.is_some());
    assert_eq!(hit.map(|h| find.offsets_for(&h, 7.0)), Some((None, None)));
    find.step(-1);
    assert_eq!(find.current().map(|h| h.line), Some(39));
    // Row 39 is above the view: scrolled so that it is in the middle.
    let hit = find.take_reveal();
    let (y, _) = hit.map_or((None, None), |h| find.offsets_for(&h, 7.0));
    assert_eq!(y, Some(39.0 * 19.0 - (190.0 - 19.0) / 2.0));
    for _ in 0..60 {
        find.step(1);
    }
    assert_eq!(find.current().map(|h| h.line), Some(99));
    find.step(1);
    assert_eq!(find.current().map(|h| h.line), Some(0));
    find.step(-1);
    assert_eq!(find.current().map(|h| h.line), Some(99));
    // Closed, nothing is marked.
    find.close();
    assert!(find.marks(5).is_none() && find.take_reveal().is_none());
}

#[test]
fn a_huge_file_is_searched_over_frames_not_in_one() {
    let ctx = egui::Context::default();
    let lines: Vec<String> = (0..400_000)
        .map(|i| format!("line number {i} of a big file"))
        .collect();
    let mut find = Find::default();
    find.show();
    find.query = query("9 of");
    let key = super::SourceKey {
        what: super::Source::DiffFile(3),
        at: lines.as_ptr() as usize,
        len: lines.len(),
    };
    let started = Instant::now();
    find.update(
        &ctx,
        key.clone(),
        lines.len(),
        |i| super::nth(&lines, i),
        super::Layout::Lines,
    );
    // One frame's share took a few milliseconds and found the first matches.
    assert!(started.elapsed() < Duration::from_millis(200));
    let first = find.summary().map(|(s, _)| s).unwrap_or_default();
    assert!(first.ends_with('…'), "{first}");
    assert_eq!(find.current().map(|h| h.line), Some(9));
    let mut frames = 1;
    while find.summary().is_some_and(|(s, _)| s.ends_with('…')) && frames < 100_000 {
        find.update(
            &ctx,
            key.clone(),
            lines.len(),
            |i| super::nth(&lines, i),
            super::Layout::Lines,
        );
        frames += 1;
    }
    assert!(frames > 1);
    assert_eq!(find.count(), 40_000);
    assert_eq!(
        find.summary().map(|(s, _)| s),
        Some("1 of 40000".to_owned())
    );
}

/// A tab drawn frame by frame as the window draws it, with the input given.
struct Window {
    ctx: egui::Context,
    tab: RepoTab,
    /// What the last frame painted.
    shapes: Vec<egui::epaint::ClippedShape>,
}

impl Window {
    fn open(dir: &Path) -> Self {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let tab = RepoTab::new(dir.to_path_buf(), &ctx);
        Self {
            ctx,
            tab,
            shapes: Vec::new(),
        }
    }

    /// A frame with `events`, `modifiers` held down.
    fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) {
        let mut events = events;
        events.insert(0, Event::ModifiersChanged(modifiers));
        let input = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let tab = &mut self.tab;
        let mut out = self.ctx.run_ui(input, |ui| tab.ui(ui));
        self.shapes = std::mem::take(&mut out.shapes);
        out.drop_without_applying_deltas();
    }

    /// Rectangles of the last frame filled with `color`.
    fn filled(&self, color: egui::Color32) -> usize {
        self.shapes
            .iter()
            .filter(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == color))
            .count()
    }

    /// Draws frames until `done` (the background reads come in meanwhile); whether it was.
    fn until(&mut self, done: impl Fn(&RepoTab) -> bool) -> bool {
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until {
            self.frame(Vec::new(), Modifiers::NONE);
            if done(&self.tab) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// A key pressed and let go, then a frame without it.
    fn key(&mut self, key: Key, modifiers: Modifiers) {
        let event = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        self.frame(vec![event(true), event(false)], modifiers);
        self.frame(Vec::new(), Modifiers::NONE);
    }

    fn type_text(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.to_owned())], Modifiers::NONE);
        self.frame(Vec::new(), Modifiers::NONE);
    }

    /// The current match: its line and first character.
    fn current(&self) -> Option<(usize, usize)> {
        self.tab.find.current().map(|h| (h.line, h.start))
    }
}

#[test]
fn ctrl_f_finds_in_the_diff_file_or_blame_shown_and_in_the_commits_otherwise() -> Result<(), String>
{
    let dir = repo("keys")?;
    let hash = head(&dir)?;
    let mut w = Window::open(&dir);
    let opened = w.until(|t| !t.opening() && t.settled());

    // The graph: Ctrl+F is the commit search.
    w.key(Key::F, Modifiers::COMMAND);
    let graph_search = (w.tab.search.open, w.tab.find.open);
    w.key(Key::Escape, Modifiers::NONE);
    let graph_closed = !w.tab.search.open;

    // A diff: the find bar, the commit search left alone.
    w.tab.queue(Action::OpenDiff(DiffKey::Commit {
        hash: hash.clone(),
        path: PATH.to_owned(),
    }));
    let diff_read = w.until(|t| matches!(&t.diff, Some(d) if matches!(d.data, Some(Ok(_)))));
    w.key(Key::F, Modifiers::COMMAND);
    let in_diff = (w.tab.find.open, w.tab.search.open);
    w.type_text("e");
    let diff_count = w.tab.find.count();
    let first = w.current();
    // Marked: the current match strongly, the others softly.
    let warning = crate::theme::Tokens::get(&w.ctx).warning;
    let painted = (
        w.filled(warning.gamma_multiply(0.6)),
        w.filled(warning.gamma_multiply(0.28)),
    );
    // Enter in the field steps on, F3 too, Shift+F3 back.
    w.key(Key::Enter, Modifiers::NONE);
    let after_enter = w.current();
    w.key(Key::F3, Modifiers::NONE);
    let after_f3 = w.current();
    w.key(Key::F3, Modifiers::SHIFT);
    let after_shift_f3 = w.current();
    // Esc closes the bar, not the diff; the next one closes the diff.
    w.key(Key::Escape, Modifiers::NONE);
    let bar_closed = (!w.tab.find.open, w.tab.diff.is_some());
    w.key(Key::Escape, Modifiers::NONE);
    let diff_closed = w.tab.diff.is_none();

    // The blame: Ctrl+F again finds in it, with the text typed before.
    w.tab.queue(Action::FileView {
        path: PATH.to_owned(),
        rev: None,
        tab: FileTab::Blame,
    });
    let blamed = w.until(|t| matches!(&t.file_view, Some(v) if matches!(v.blame, Some(Ok(_)))));
    w.key(Key::F, Modifiers::COMMAND);
    w.until(|t| t.find.summary().is_some());
    let in_blame = (
        w.tab.find.open,
        w.tab.find.query.text.clone(),
        w.tab.find.count(),
    );
    // The file: the same, from its own lines.
    w.tab.queue(Action::FileView {
        path: PATH.to_owned(),
        rev: None,
        tab: FileTab::File,
    });
    let read = w.until(|t| matches!(&t.file_view, Some(v) if matches!(v.text, Some(Ok(_)))));
    // Ctrl+F selects what was typed: typing replaces it.
    w.key(Key::F, Modifiers::COMMAND);
    w.type_text("omega");
    let in_file = w.tab.find.count();
    // History has nothing to search: back to the commit search.
    w.tab.queue(Action::FileView {
        path: PATH.to_owned(),
        rev: None,
        tab: FileTab::History,
    });
    w.frame(Vec::new(), Modifiers::NONE);
    w.frame(Vec::new(), Modifiers::NONE);
    let left = !w.tab.find.open;
    let _ = std::fs::remove_dir_all(&dir);

    assert!(opened, "the repository never opened");
    assert_eq!(graph_search, (true, false));
    assert!(graph_closed);
    assert!(diff_read, "the diff never came");
    assert_eq!(in_diff, (true, false));
    // "e": "Grüße" and "GRÜSSE" (1 each), "needle b" and "Needle A2" (3 each), "omega" (1).
    assert_eq!(diff_count, 9);
    assert!(first.is_some());
    assert_eq!(painted, (1, diff_count - 1));
    assert_ne!(after_enter, first);
    assert_ne!(after_f3, after_enter);
    assert_eq!(after_shift_f3, after_enter);
    assert_eq!(bar_closed, (true, true));
    assert!(diff_closed);
    assert!(blamed, "the blame never came");
    // HEAD's file: "GRÜSSE" (1), "Needle A2" (3), "plain B2" (0), "omega" (1).
    assert_eq!(in_blame, (true, "e".to_owned(), 5));
    assert!(read, "the file never came");
    assert_eq!(in_file, 1);
    assert!(left);
    Ok(())
}
