//! Tests of the rich views' frame (reading both sides, going back to a line) and of prose.

use std::collections::HashSet;

use super::prose::{self, Part};
use super::prose_view::{Item, layout};
use super::words::{self, Change};
use super::*;
use crate::repo::diff::{Kind, parse};

fn sides(old: &str, new: &str) -> Sides {
    Sides {
        old: Some(old.to_owned()),
        new: Some(new.to_owned()),
    }
}

#[test]
fn words_are_letters_and_digits_and_each_mark_on_its_own() {
    assert_eq!(
        words::words("Héllo, wörld! It's 3.5 — fine_ok\tnow"),
        [
            "Héllo", ",", "wörld", "!", "It's", "3", ".", "5", "—", "fine_ok", "now"
        ]
    );
    assert!(words::words("  \n\t ").is_empty());
    assert_eq!(words::words("日本語 テキスト"), ["日本語", "テキスト"]);
}

#[test]
fn a_word_diff_keeps_the_common_words() {
    let old = words::words("the quick brown fox jumps");
    let new = words::words("the slow brown fox leaps high");
    let d = words::diff(&old, &new);
    let kept: Vec<&str> = d
        .iter()
        .filter(|(c, _)| *c == Change::Kept)
        .map(|(_, w)| *w)
        .collect();
    assert_eq!(kept, ["the", "brown", "fox"]);
    assert!(d.contains(&(Change::Removed, "quick")));
    assert!(d.contains(&(Change::Added, "high")));
    assert_eq!(words::similarity(&old, &old), 1.0);
    assert_eq!(words::similarity(&[], &[]), 1.0);
}

#[test]
fn a_paragraph_only_rewrapped_is_kept_and_says_so() {
    let old = "Title\n\nA long paragraph that\nwraps here.\n";
    let new = "Title\n\nA long paragraph\nthat wraps here.\n";
    let d = prose::diff(&sides(old, new));
    assert_eq!(d.parts.len(), 2);
    assert!(matches!(
        &d.parts[1],
        Part::Kept {
            rewrapped: true,
            old: 3,
            new: 3,
            ..
        }
    ));
}

#[test]
fn an_edited_paragraph_shows_its_changed_words() {
    let old = "Intro stays.\n\nThe cat sat on\nthe mat today.\n\nOutro stays.\n";
    let new = "Intro stays.\n\nThe dog sat on the\nmat today.\n\nA brand new paragraph here.\n\nOutro stays.\n";
    let d = prose::diff(&sides(old, new));
    let kinds: Vec<&str> = d
        .parts
        .iter()
        .map(|p| match p {
            Part::Kept { .. } => "kept",
            Part::Removed { .. } => "removed",
            Part::Added { .. } => "added",
            Part::Changed { .. } => "changed",
        })
        .collect();
    assert_eq!(kinds, ["kept", "changed", "added", "kept"]);
    let Part::Changed { words, old, new } = &d.parts[1] else {
        return;
    };
    assert_eq!((*old, *new), (3, 3));
    assert!(words.contains(&(Change::Removed, " cat".to_owned())));
    assert!(words.contains(&(Change::Added, " dog".to_owned())));
    assert!(words.iter().filter(|(c, _)| *c != Change::Kept).count() == 2);
}

#[test]
fn unrelated_paragraphs_are_removed_and_added_not_paired() {
    let d = prose::diff(&sides(
        "Alpha beta gamma.\n",
        "Completely different words entirely.\n",
    ));
    assert!(matches!(d.parts[0], Part::Removed { .. }));
    assert!(matches!(d.parts[1], Part::Added { .. }));
}

#[test]
fn added_and_deleted_files_and_crlf_and_empty_text() {
    let added = prose::diff(&Sides {
        old: None,
        new: Some("One.\r\n\r\nTwo.\r\n".to_owned()),
    });
    assert_eq!(added.parts.len(), 2);
    assert!(matches!(&added.parts[1], Part::Added { new: 3, text } if text == "Two."));
    let deleted = prose::diff(&Sides {
        old: Some("Gone.\n".to_owned()),
        new: None,
    });
    assert!(matches!(deleted.parts[0], Part::Removed { old: 1, .. }));
    assert!(prose::diff(&sides("", "")).parts.is_empty());
}

#[test]
fn long_runs_of_unchanged_paragraphs_fold_and_open() {
    let kept = |n: u32| Part::Kept {
        old: n,
        new: n,
        text: "x".to_owned(),
        rewrapped: false,
    };
    let added = Part::Added {
        new: 99,
        text: "y".to_owned(),
    };
    let mut parts: Vec<Part> = (0..10).map(kept).collect();
    parts.push(added.clone());
    parts.extend((20..30).map(kept));
    let folded = layout(&parts, &HashSet::new());
    // Before the change: one paragraph kept in view; after it, one too.
    assert_eq!(folded[0], Item::Fold { run: 0, count: 9 });
    assert_eq!(folded[1], Item::Part(9));
    assert_eq!(folded[2], Item::Part(10));
    assert_eq!(folded[3], Item::Part(11));
    assert_eq!(folded[4], Item::Fold { run: 11, count: 9 });
    let opened = layout(&parts, &HashSet::from([0]));
    assert_eq!(
        opened.iter().filter(|i| matches!(i, Item::Part(_))).count(),
        12
    );
    // Nothing changed at all: everything shows.
    let same: Vec<Part> = (0..10).map(kept).collect();
    assert_eq!(layout(&same, &HashSet::new()).len(), 10);
}

#[test]
fn a_line_goes_back_to_its_row_or_the_nearest() {
    let lines = parse("@@ -3,3 +3,3 @@\n a\n-b\n+B\n c\n@@ -20,1 +20,1 @@\n-x\n+y\n");
    assert_eq!(tab::line_index(&lines, Some(4), Some(4)), Some(3));
    assert_eq!(tab::line_index(&lines, Some(4), None), Some(2));
    assert_eq!(tab::line_index(&lines, None, Some(20)), Some(7));
    // Not in the diff: the nearest line shown.
    let near = tab::line_index(&lines, None, Some(18)).map(|i| lines[i].new);
    assert_eq!(near, Some(Some(20)));
    assert_eq!(tab::line_index(&[], None, Some(1)), None);
    assert!(lines.iter().any(|l| l.kind == Kind::Hunk));
}

#[test]
fn kinds_follow_the_extension() {
    for path in ["README.md", "docs/guide.ADOC", "notes.txt", "a b/ñ.rst"] {
        assert_eq!(RichKind::of(path), [RichKind::Prose], "{path}");
    }
    assert!(RichKind::of("main.rs").is_empty());
    assert!(RichKind::of("Makefile").is_empty());
}

/// Both sides are read from a real repository, however the change touched the file; a file
/// that isn't UTF-8 falls back to the plain diff with a note.
#[test]
fn sides_are_read_from_commits_the_index_and_the_working_tree() -> Result<(), String> {
    use crate::git::{self, Area, StatusFile};
    let dir = std::env::temp_dir().join(format!("gitcraft-rich-sides-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    let name = "dös notes.md";
    let _ = std::fs::write(dir.join(name), "First.\n\nSecond.\n");
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-qm", "one"])?;
    let _ = std::fs::write(dir.join(name), "First.\n\nSecond, edited.\n");
    let work = |area| {
        DiffKey::Work(StatusFile {
            path: name.to_owned(),
            old_path: None,
            code: 'M',
            area,
        })
    };
    let unstaged = read_sides(&dir, &work(Area::Unstaged), None);
    git::run(Some(&dir), &["add", "-A"])?;
    let staged = read_sides(&dir, &work(Area::Staged), None);
    git::run(Some(&dir), &["commit", "-qm", "two"])?;
    let head = git::head(&dir);
    let commit = read_sides(
        &dir,
        &DiffKey::Commit {
            hash: head.clone(),
            path: name.to_owned(),
        },
        None,
    );
    let first = git::run(Some(&dir), &["rev-list", "--max-parents=0", "HEAD"])?;
    let root = read_sides(
        &dir,
        &DiffKey::Commit {
            hash: first.trim().to_owned(),
            path: name.to_owned(),
        },
        None,
    );
    let _ = std::fs::write(dir.join("latin.txt"), b"caf\xe9\n");
    let latin = load(
        &dir,
        &DiffKey::Work(StatusFile {
            path: "latin.txt".to_owned(),
            old_path: None,
            code: '?',
            area: Area::Untracked,
        }),
        RichKind::Prose,
        None,
    );
    let _ = std::fs::remove_dir_all(&dir);
    let edited = Sides {
        old: Some("First.\n\nSecond.\n".to_owned()),
        new: Some("First.\n\nSecond, edited.\n".to_owned()),
    };
    assert_eq!(unstaged, Ok(edited.clone()));
    assert_eq!(staged, Ok(edited.clone()));
    assert_eq!(commit, Ok(edited));
    // The first commit added the file: no old side.
    assert_eq!(root.map(|s| s.old), Ok(None));
    assert!(matches!(latin, Ok(Rich::Plain(note)) if note.contains("UTF-8")));
    Ok(())
}

/// The prose view shows the changed words and a click goes back to the diff.
#[test]
fn the_prose_view_shows_changes_and_goes_back_to_the_line() {
    use egui_kittest::kittest::Queryable;
    let d = prose::diff(&sides(
        "Intro.\n\nThe cat sat.\n",
        "Intro.\n\nThe dog sat.\n\nA new one.\n",
    ));
    let diff = RichDiff::Prose(d);
    let acts = std::cell::RefCell::new(Vec::new());
    let state = std::cell::RefCell::new(RichState::default());
    let fonts = std::cell::Cell::new(false);
    let mut harness = egui_kittest::Harness::builder()
        .with_size([800.0, 500.0])
        .build_ui(|ui| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            show(ui, &diff, &mut state.borrow_mut(), &mut acts.borrow_mut());
        });
    harness.run();
    assert!(harness.query_by_label_contains("A new one.").is_some());
    harness.get_by_label_contains("A new one.").click();
    harness.run();
    assert!(acts.borrow().iter().any(|a| matches!(
        a,
        Action::DiffGoToLine {
            old: None,
            new: Some(5)
        }
    )));
}

/// An edited paragraph reads as written: abbreviations, paths and links stay whole.
#[test]
fn edited_text_keeps_its_own_spacing() {
    let old = "See e.g. https://x.com/a/b and [the docs](d.md), please.";
    let new = "See e.g. https://x.com/a/c and [the docs](d.md), thanks.";
    let words = words::diff_text(old, new);
    let new_side: String = words
        .iter()
        .filter(|(c, _)| *c != Change::Removed)
        .map(|(_, w)| w.as_str())
        .collect();
    assert_eq!(new_side, new);
    let old_side: String = words
        .iter()
        .filter(|(c, _)| *c != Change::Added)
        .map(|(_, w)| w.as_str())
        .collect();
    assert_eq!(old_side, old);
    // Re-wrapped lines read as one flow; a list item keeps its line.
    let flowed: String = words::diff_text("a\nb", "a\nb\n- item")
        .into_iter()
        .map(|(_, w)| w)
        .collect();
    assert_eq!(flowed, "a b\n- item");
}

/// One word changed in a paragraph of thousands is still that paragraph edited.
#[test]
fn a_small_edit_in_a_long_paragraph_is_still_an_edit() {
    let old: String = (0..3000).map(|n| format!("word{n} ")).collect();
    let new = old.replace("word1500 ", "changed ");
    let d = prose::diff(&sides(&old, &new));
    assert_eq!(d.parts.len(), 1);
    let Part::Changed { words, .. } = &d.parts[0] else {
        return;
    };
    assert_eq!(words.iter().filter(|(c, _)| *c != Change::Kept).count(), 2);
}

#[test]
fn a_file_missing_on_a_side_is_told_from_a_failure() {
    assert!(missing("fatal: path 'a.md' does not exist in 'abc^'"));
    assert!(missing("fatal: invalid object name 'abc^'."));
    assert!(missing("a.md: No such file or directory (os error 2)"));
    assert!(!missing(
        "ssh: connect to host example.com port 22: Connection refused"
    ));
}

#[test]
fn combining_marks_stay_in_their_word() {
    assert_eq!(words::words("Cafe\u{301} ok"), ["Cafe\u{301}", "ok"]);
    assert_eq!(words::words("नमस्ते दुनिया"), ["नमस्ते", "दुनिया"]);
    assert_eq!(words::words("\u{feff}Hello"), ["Hello"]);
    assert_eq!(words::words("a—b"), ["a", "—", "b"]);
}

/// Many long paragraphs all rewritten don't take seconds to pair.
#[test]
fn pairing_a_huge_rewrite_stays_bounded() {
    let para = |seed: usize| -> String {
        (0..1900)
            .map(|n| format!("w{}x{} ", seed, n))
            .collect::<String>()
            + "\n\n"
    };
    let old: String = (0..120).map(para).collect();
    let new: String = (1000..1120).map(para).collect();
    let started = std::time::Instant::now();
    let d = prose::diff(&sides(&old, &new));
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    assert_eq!(d.parts.len(), 240);
}

/// A file renamed and edited in a commit reads its old side under its old name.
#[test]
fn a_renamed_file_reads_its_old_name_before_the_change() -> Result<(), String> {
    use crate::git::{self, DiffOptions};
    let dir = std::env::temp_dir().join(format!("gitcraft-rich-rename-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    let body: String = (0..30)
        .map(|n| format!("Paragraph number {n} of the guide.\n\n"))
        .collect();
    let _ = std::fs::write(dir.join("old.md"), &body);
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-qm", "one"])?;
    git::run(Some(&dir), &["mv", "old.md", "new.md"])?;
    let _ = std::fs::write(dir.join("new.md"), body.replace("number 7", "number seven"));
    git::run(Some(&dir), &["commit", "-qam", "two"])?;
    let key = DiffKey::Commit {
        hash: git::head(&dir),
        path: "new.md".to_owned(),
    };
    let data = crate::repo::diff::load(&dir, &key, DiffOptions::default(), true)?;
    let old_path = super::super::images::renamed_from(&data.header).map(str::to_owned);
    let rich = load(&dir, &key, RichKind::Prose, old_path.as_deref());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(old_path.as_deref(), Some("old.md"));
    let Ok(Rich::Ready(RichDiff::Prose(d))) = rich else {
        return Err("not read as prose".to_owned());
    };
    let edited = d
        .parts
        .iter()
        .filter(|p| !matches!(p, Part::Kept { .. }))
        .count();
    assert_eq!(edited, 1);
    Ok(())
}
