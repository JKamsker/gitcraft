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
    let _ = search_diff(&mut find, &ctx, PATH, &data, false);
    // Unified: the removed line comes first.
    assert_eq!(stepped(&mut find), [(removed, 0), (added, 0)]);
    // Side by side: the added line is on the row above, on the right. The current match
    // stays the one it was.
    let before = find.current().map(|h| h.line);
    let _ = search_diff(&mut find, &ctx, PATH, &data, true);
    assert_eq!(find.current().map(|h| h.line), before);
    assert_eq!(stepped(&mut find), [(added, 0), (removed, 0)]);

    // Case folds beyond ASCII ("ß" matches "SS" only as a full fold, which isn't done).
    find.query = query("grüße");
    let _ = search_diff(&mut find, &ctx, PATH, &data, false);
    assert_eq!(find.count(), 1);
    find.query.case_sensitive = true;
    find.query.text = "Needle".to_owned();
    let _ = search_diff(&mut find, &ctx, PATH, &data, false);
    assert_eq!(stepped(&mut find), [(added, 0)]);
    // Hunk headers aren't searched.
    find.query = query("@@");
    let _ = search_diff(&mut find, &ctx, PATH, &data, false);
    assert_eq!(
        (find.count(), find.summary()),
        (0, Some(("no results".into(), false)))
    );
    // A broken regular expression says so.
    find.query = query("needle(");
    find.query.regex = true;
    let _ = search_diff(&mut find, &ctx, PATH, &data, false);
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
    assert_eq!(
        hit.map(|h| find.offsets_for(&h, "", 7.0)),
        Some((None, None))
    );
    find.step(-1);
    assert_eq!(find.current().map(|h| h.line), Some(39));
    // Row 39 is above the view: scrolled so that it is in the middle.
    let hit = find.take_reveal();
    let (y, _) = hit.map_or((None, None), |h| find.offsets_for(&h, "", 7.0));
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
        what: super::Source::DiffFile("big.txt".into()),
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
        Some("1 of 40,000".to_owned())
    );
}

// A tab drawn frame by frame as the window draws it, with the input given.
include!("tests_window.rs");
include!("tests_source.rs");
include!("tests_keyboard.rs");
