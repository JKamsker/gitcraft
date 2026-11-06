//! Comparing pictures through the diff the view loads, on real repositories.

use std::path::{Path, PathBuf};

use super::decode::tests::png;
use super::parse_history;
use super::view::changes;
use crate::git::{self, Area, DiffOptions, StatusFile};
use crate::repo::DiffKey;
use crate::repo::diff::load;

fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-imgcmp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    Ok(dir)
}

fn commit(dir: &Path, name: &str, bytes: &[u8], message: &str) -> Result<String, String> {
    if let Some(parent) = dir.join(name).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(dir.join(name), bytes).map_err(|e| e.to_string())?;
    git::run(Some(dir), &["add", "-A"])?;
    git::run(Some(dir), &["commit", "-qm", message])?;
    Ok(git::head(dir))
}

#[test]
fn history_records_read() {
    let out = "\x1eaaa\x1f100\x1fRecolour, again\n\nart/new name.png\n\x1ebbb\x1f50\x1fFirst\n\nold.png\n";
    let h = parse_history(out, "art/new name.png");
    let short: Vec<(&str, i64, &str, &str)> = h
        .iter()
        .map(|v| (v.hash.as_str(), v.time, v.summary.as_str(), v.path.as_str()))
        .collect();
    assert_eq!(
        short,
        [
            ("aaa", 100, "Recolour, again", "art/new name.png"),
            ("bbb", 50, "First", "old.png")
        ]
    );
    assert!(parse_history("", "x").is_empty());
}

/// A picture recoloured and resized in a commit: both decoded, the difference counted, the
/// change worded, and its history (through a rename) listed newest first.
#[test]
fn a_picture_changed_in_a_commit() -> Result<(), String> {
    let dir = repo("commit")?;
    commit(&dir, "logo.png", &png(4, 4, [255, 0, 0, 255]), "red")?;
    std::fs::create_dir_all(dir.join("art ü")).map_err(|e| e.to_string())?;
    git::run(Some(&dir), &["mv", "logo.png", "art ü/logo.png"])?;
    git::run(Some(&dir), &["commit", "-qm", "move"])?;
    let hash = commit(
        &dir,
        "art ü/logo.png",
        &png(4, 2, [255, 0, 0, 128]),
        "smaller, see-through",
    )?;
    let key = DiffKey::Commit {
        hash: hash.clone(),
        path: "art ü/logo.png".into(),
    };
    let data = load(&dir, &key, DiffOptions::default(), true)?;
    let cmp = data.compare.ok_or("not compared")?;
    // A thumbnail of the oldest, under its old name.
    let oldest = cmp.history.last().ok_or("no history")?;
    let thumb = super::filmstrip::read(&cmp.repo, &oldest.hash, &oldest.path);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(thumb.map(|t| t.size), Some([4, 4]));
    let dims = |d: &Option<super::decode::Decoded>| d.as_ref().map(|d| (d.width, d.height));
    assert_eq!(
        (dims(&cmp.old), dims(&cmp.new)),
        (Some((4, 4)), Some((4, 2)))
    );
    let diff = cmp.diff.as_ref().ok_or("no difference")?;
    // Every pixel: the top half changed its alpha, the bottom half is only in the old one.
    assert_eq!((diff.changed, diff.total), (16, 16));
    let text = changes(&cmp);
    assert!(text.starts_with("4 × 4 → 4 × 2 · "), "{text}");
    assert!(text.ends_with("100.0% of pixels changed"), "{text}");
    let history: Vec<(&str, &str)> = cmp
        .history
        .iter()
        .map(|v| (v.summary.as_str(), v.path.as_str()))
        .collect();
    assert_eq!(
        history,
        [
            ("smaller, see-through", "art ü/logo.png"),
            ("move", "art ü/logo.png"),
            ("red", "logo.png")
        ]
    );
    assert_eq!(cmp.current.as_deref(), Some(hash.as_str()));
    Ok(())
}

/// A new picture in the working tree: no old side; the same picture again: no pixel changed;
/// bytes that aren't a picture: no comparison, the plain view instead.
#[test]
fn new_unchanged_and_broken_pictures() -> Result<(), String> {
    let dir = repo("work")?;
    std::fs::write(dir.join("new.png"), png(2, 3, [0, 0, 0, 0])).map_err(|e| e.to_string())?;
    let key = DiffKey::Work(StatusFile {
        path: "new.png".into(),
        old_path: None,
        code: '?',
        area: Area::Untracked,
    });
    let data = load(&dir, &key, DiffOptions::default(), true)?;
    let cmp = data.compare.ok_or("new picture not compared")?;
    assert!(cmp.old.is_none() && cmp.diff.is_none());
    assert_eq!(changes(&cmp).split(" · ").next(), Some("2 × 3"));

    commit(&dir, "same.png", &png(2, 2, [1, 2, 3, 255]), "one")?;
    // The same pixels written again (another file, as an editor might save it).
    let mut again = png(2, 2, [1, 2, 3, 255]);
    again.extend(b"trailing");
    let hash = commit(&dir, "same.png", &again, "saved again")?;
    let key = DiffKey::Commit {
        hash,
        path: "same.png".into(),
    };
    let cmp = load(&dir, &key, DiffOptions::default(), true)?
        .compare
        .ok_or("not compared")?;
    assert!(changes(&cmp).contains("same dimensions"));
    assert!(
        changes(&cmp).ends_with("no pixel changed"),
        "{}",
        changes(&cmp)
    );

    let hash = commit(&dir, "broken.png", b"not a png at all", "broken")?;
    let key = DiffKey::Commit {
        hash,
        path: "broken.png".into(),
    };
    let data = load(&dir, &key, DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(data.compare.is_none());
    assert!(data.image.is_some());
    Ok(())
}
