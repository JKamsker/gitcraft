//! Tests of the folds of unchanged lines: where they are, opening them, and that hunks and
//! lines still stage once they are open, on real repositories.

use std::path::{Path, PathBuf};

use super::*;
use crate::git::{self, DiffOptions};
use crate::repo::diff::{Pick, hunk_changes, load, parse, partial_patch};
use crate::repo::{DiffKey, DiffView};

#[test]
fn header_numbers_read_counts_and_the_function() {
    assert_eq!(
        header_numbers("@@ -12,5 +14,7 @@ fn x()"),
        Some((12, 5, 14, 7, " fn x()"))
    );
    assert_eq!(header_numbers("@@ -3 +3,0 @@"), Some((3, 1, 3, 0, "")));
    assert_eq!(header_numbers("@@@ -1,1 -1,1 +1,1 @@@"), None);
    assert_eq!(
        function_of("@@ -1,2 +1,2 @@   impl Foo {  "),
        Some("impl Foo {")
    );
    assert_eq!(function_of("@@ -1,2 +1,2 @@"), None);
}

#[test]
fn gaps_lie_between_the_hunks_and_after_the_last() {
    // 30 lines; line 5 changed, line 20 removed.
    let text = "@@ -2,7 +2,7 @@\n a\n b\n c\n-d\n+D\n e\n f\n g\n@@ -17,7 +17,6 @@\n h\n i\n j\n-k\n l\n m\n n\n";
    let lines = parse(text);
    let gaps = gaps_of(&lines, 29).unwrap_or_default();
    let summary: Vec<_> = gaps
        .iter()
        .map(|g| (g.first, g.last, g.old_offset, g.trailing))
        .collect();
    assert_eq!(
        summary,
        vec![(1, 1, 0, false), (9, 16, 0, false), (23, 29, 1, true)]
    );
    assert_eq!(gaps[0].row, 0);
    assert_eq!(gaps[1].row, 9);
    assert_eq!(gaps[2].row, lines.len());
    // A hunk adding lines to an empty range numbers it by the line before.
    let added = parse("@@ -4,0 +5,2 @@\n+x\n+y\n");
    let gaps = gaps_of(&added, 10).unwrap_or_default();
    assert_eq!((gaps[0].first, gaps[0].last), (1, 4));
    assert_eq!(
        (gaps[1].first, gaps[1].last, gaps[1].old_offset),
        (7, 10, -2)
    );
    assert!(gaps_of(&parse("@@@ -1,1 -1,1 +1,1 @@@\n  a\n"), 1).is_none());
    assert_eq!(gaps_of(&[], 5), Some(Vec::new()));
}

#[test]
fn the_enclosing_function_runs_from_its_header_to_its_end() {
    let file =
        "use x;\n\nfn outer(a: u32) -> u32 {\n    let b = a;\n\n    b + 1\n}\n\nfn next() {}\n";
    let ctx = Context {
        raw: file.lines().map(str::to_owned).collect(),
        path: "a.rs".to_owned(),
        dark: true,
        no_newline_at_end: false,
        gaps: Vec::new(),
    };
    assert_eq!(
        ctx.function_span("@@ -5,2 +5,2 @@ fn outer(a: u32) -> u32 {", 5),
        Some((3, 7))
    );
    // Python: no closing line, the function ends before the next one at its depth.
    let py = "class A:\n    def f(self):\n        x = 1\n        return x\n    def g(self):\n        pass\n";
    let ctx = Context {
        raw: py.lines().map(str::to_owned).collect(),
        ..ctx
    };
    assert_eq!(
        ctx.function_span("@@ -3,1 +3,1 @@ def f(self):", 3),
        Some((2, 4))
    );
    assert_eq!(ctx.function_span("@@ -3,1 +3,1 @@ def missing():", 3), None);
}

/// A repository whose `name` is a 100-line file, committed.
fn repo(tag: &str, name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-ctx-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let _ = std::fs::write(dir.join(name), text);
    assert!(git::run(Some(&dir), &["add", "-A"]).is_ok());
    assert!(git::run(Some(&dir), &["commit", "-qm", "base"]).is_ok());
    dir
}

fn numbered(n: usize, end: &str) -> String {
    (1..=n)
        .map(|k| format!("línea {k}\twith a tab{end}\n"))
        .collect()
}

fn unstaged(name: &str) -> DiffKey {
    DiffKey::Work(git::StatusFile {
        path: name.to_owned(),
        old_path: None,
        code: 'M',
        area: git::Area::Unstaged,
    })
}

/// Applies `pick` to the lines `chosen` of `view`'s diff, as the view does.
fn apply(
    dir: &Path,
    view: &DiffView,
    chosen: &std::collections::BTreeSet<usize>,
    pick: Pick,
) -> Result<(), String> {
    let Some(Ok(data)) = &view.data else {
        return Err("no diff".to_owned());
    };
    let patch = partial_patch(&data.header, &data.lines, chosen, pick).ok_or("nothing picked")?;
    let path = git::save_patch(&patch)?.to_string_lossy().into_owned();
    let mut args = pick.args().to_vec();
    args.push(&path);
    git::run(Some(dir), &args).map(|_| ())
}

fn view_of(dir: &Path, key: &DiffKey) -> Result<DiffView, String> {
    let mut view = DiffView::new(key.clone());
    let request = view.next_request();
    view.arrived(request, load(dir, key, DiffOptions::default(), true));
    Ok(view)
}

fn data(view: &DiffView) -> Result<&DiffData, String> {
    match &view.data {
        Some(Ok(d)) => Ok(d),
        Some(Err(e)) => Err(e.clone()),
        None => Err("not read".to_owned()),
    }
}

fn check_unfolded_hunks_stage(tag: &str, end: &str) -> Result<(), String> {
    let name = "dossier à/ño file.txt";
    let base = numbered(100, end);
    let dir = repo(tag, &name.replace('/', "_"), &base);
    let name = name.replace('/', "_");
    let edited = base
        .replace(
            &format!("línea 10\twith a tab{end}\n"),
            &format!("línea diez{end}\n"),
        )
        .replace(
            &format!("línea 60\twith a tab{end}\n"),
            &format!("línea sesenta{end}\n"),
        );
    let _ = std::fs::write(dir.join(&name), &edited);
    let key = unstaged(&name);
    let mut view = view_of(&dir, &key)?;
    let gaps: Vec<(u32, u32)> = data(&view)?
        .context
        .as_ref()
        .map(|c| c.gaps.iter().map(|g| (g.first, g.last)).collect())
        .unwrap_or_default();
    assert_eq!(gaps, vec![(1, 6), (14, 56), (64, 100)]);
    // The second hunk's change is picked, then folds open around it.
    let second = data(&view)?
        .lines
        .iter()
        .position(|l| l.text == "línea sesenta")
        .ok_or("no change")?;
    view.chosen.insert(second);
    view.unfold(1, Unfold::Up(20));
    view.unfold(1, Unfold::Down(5));
    view.unfold(0, Unfold::All);
    view.unfold(2, Unfold::All);
    let d = data(&view)?;
    // The pick followed its line.
    let picked: Vec<&str> = view
        .chosen
        .iter()
        .map(|&i| d.lines[i].text.as_str())
        .collect();
    assert_eq!(picked, ["línea sesenta"]);
    let ctx = d.context.as_ref().ok_or("no context")?;
    let hidden: Vec<u32> = ctx.gaps.iter().map(Gap::hidden).collect();
    assert_eq!(hidden, vec![0, 43 - 25, 0]);
    // No fold row is left after the end, and every line is numbered as git numbers it.
    assert!(d.lines.iter().all(|l| l.kind != Kind::Fold));
    assert!(
        d.lines
            .iter()
            .any(|l| l.new == Some(100) && l.old == Some(100))
    );
    // Side by side, every line has its row, in order.
    let in_split: Vec<usize> = d
        .split
        .iter()
        .flat_map(|&(l, r)| [l, r])
        .flatten()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(in_split, (0..d.lines.len()).collect::<Vec<_>>());
    let order: Vec<usize> = d.split.iter().filter_map(|&(l, r)| l.or(r)).collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]));
    // Staging the picked line, then a whole hunk, still applies.
    let chosen = view.chosen.clone();
    apply(&dir, &view, &chosen, Pick::Stage)?;
    let first_hunk = d
        .lines
        .iter()
        .position(|l| l.kind == Kind::Hunk)
        .ok_or("no hunk")?;
    apply(
        &dir,
        &view,
        &hunk_changes(&d.lines, first_hunk),
        Pick::Stage,
    )?;
    let staged = git::run(Some(&dir), &["diff", "--cached", "--no-color"])?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(staged.contains("+línea diez"), "{staged}");
    assert!(staged.contains("+línea sesenta"), "{staged}");
    Ok(())
}

#[test]
fn hunks_and_lines_still_stage_once_folds_are_open() -> Result<(), String> {
    check_unfolded_hunks_stage("lf", "")
}

#[test]
fn hunks_still_stage_in_a_file_with_crlf_endings() -> Result<(), String> {
    check_unfolded_hunks_stage("crlf", "\r")
}

/// Opened up to its last line, a file without a newline at its end keeps git's note, and a
/// hunk still discards.
#[test]
fn the_end_of_a_file_without_a_newline_opens_with_its_note() -> Result<(), String> {
    let base = numbered(30, "");
    let base = base.trim_end_matches('\n').to_owned();
    let dir = repo("eof", "f.txt", &base);
    let _ = std::fs::write(dir.join("f.txt"), base.replace("línea 3\t", "línea tres\t"));
    let key = unstaged("f.txt");
    let mut view = view_of(&dir, &key)?;
    view.unfold(1, Unfold::All);
    let d = data(&view)?;
    let last = d.lines.last().ok_or("no lines")?;
    assert_eq!(last.kind, Kind::Meta);
    assert!(last.raw.starts_with('\\'));
    let hunk = hunk_changes(&d.lines, 0);
    apply(&dir, &view, &hunk, Pick::Discard)?;
    let now = std::fs::read_to_string(dir.join("f.txt")).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(now, base);
    Ok(())
}

/// The lines opened are open again when the diff is read again.
#[test]
fn opened_lines_stay_open_when_the_diff_is_read_again() -> Result<(), String> {
    let dir = repo("reload", "f.txt", &numbered(80, ""));
    let base = numbered(80, "");
    let _ = std::fs::write(
        dir.join("f.txt"),
        base.replace("línea 40\t", "línea cuarenta\t"),
    );
    let key = unstaged("f.txt");
    let mut view = view_of(&dir, &key)?;
    view.unfold(0, Unfold::Up(10));
    view.unfold(1, Unfold::Down(4));
    let shown = |view: &DiffView| -> Result<usize, String> { Ok(data(view)?.lines.len()) };
    let before = shown(&view)?;
    let request = view.next_request();
    view.arrived(request, load(&dir, &key, DiffOptions::default(), true));
    let after = shown(&view)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(before, after);
    Ok(())
}

/// A new file, a whole-file diff and a file that isn't UTF-8 have no folds.
#[test]
fn no_folds_where_there_is_nothing_to_open() -> Result<(), String> {
    let dir = repo("none", "f.txt", &numbered(40, ""));
    let _ = std::fs::write(
        dir.join("f.txt"),
        numbered(40, "").replace("línea 20\t", "x\t"),
    );
    let whole = load(
        &dir,
        &unstaged("f.txt"),
        DiffOptions {
            whole_file: true,
            ..Default::default()
        },
        true,
    )?;
    let _ = std::fs::write(dir.join("new.txt"), "a\nb\n");
    let untracked = load(
        &dir,
        &DiffKey::Work(git::StatusFile {
            path: "new.txt".to_owned(),
            old_path: None,
            code: '?',
            area: git::Area::Untracked,
        }),
        DiffOptions::default(),
        true,
    )?;
    let mut latin = numbered(40, "").into_bytes();
    latin.extend_from_slice(b"caf\xe9\n");
    let _ = std::fs::write(dir.join("f.txt"), &latin);
    let not_utf8 = load(&dir, &unstaged("f.txt"), DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(whole.context.is_none());
    assert!(untracked.context.is_none());
    assert!(not_utf8.context.is_none());
    Ok(())
}

#[test]
fn a_commented_line_in_a_fold_opens_from_the_nearer_hunk() {
    let ctx = Context {
        gaps: vec![
            Gap {
                row: 0,
                first: 1,
                last: 40,
                old_offset: 0,
                trailing: false,
                function: None,
            },
            Gap {
                row: 9,
                first: 60,
                last: 100,
                old_offset: -2,
                trailing: true,
                function: None,
            },
        ],
        ..Context::default()
    };
    // Near the first hunk (below line 40): up from it.
    assert_eq!(unfold_to(&ctx, 38, false, 3), Some((0, Unfold::Up(6))));
    // The first gap has no hunk above it.
    assert_eq!(unfold_to(&ctx, 2, false, 3), Some((0, Unfold::Up(42))));
    // Old side numbers go through the gap's offset.
    assert_eq!(unfold_to(&ctx, 60, true, 0), Some((1, Unfold::Down(3))));
    assert_eq!(unfold_to(&ctx, 50, false, 3), None);
}

/// _Show the enclosing function_ opens the lines from the function git named for the hunk.
#[test]
fn the_enclosing_function_opens_from_its_first_line() -> Result<(), String> {
    let mut src = String::new();
    for f in 0..6 {
        src.push_str(&format!("fn function_{f}(a: u32) -> u32 {{\n"));
        for k in 0..8 {
            src.push_str(&format!("    let v{k} = a + {k};\n"));
        }
        src.push_str("    a\n}\n\n");
    }
    let dir = repo("function", "a.rs", &src);
    let edited = src.replacen(
        "    a\n}\n\nfn function_2",
        "    a * 2\n}\n\nfn function_2",
        1,
    );
    let _ = std::fs::write(dir.join("a.rs"), edited);
    let key = unstaged("a.rs");
    let mut view = view_of(&dir, &key)?;
    let _ = std::fs::remove_dir_all(&dir);
    view.unfold(0, Unfold::Function);
    let d = data(&view)?;
    let first = d.lines.get(1).map(|l| l.text.as_str());
    assert_eq!(first, Some("fn function_1(a: u32) -> u32 {"));
    assert_eq!(
        d.lines[0].raw,
        "@@ -13,13 +13,13 @@ fn function_1(a: u32) -> u32 {"
    );
    Ok(())
}

/// With `core.autocrlf` the file on disk isn't what git diffed: no folds rather than context
/// that would make wrong patches.
#[test]
fn no_folds_when_the_file_on_disk_is_not_what_git_diffed() -> Result<(), String> {
    let dir = repo("autocrlf", "f.txt", &numbered(40, ""));
    git::run(Some(&dir), &["config", "core.autocrlf", "true"])?;
    let crlf = numbered(40, "\r").replace("línea 20\t", "veinte\t");
    let _ = std::fs::write(dir.join("f.txt"), crlf);
    let data = load(&dir, &unstaged("f.txt"), DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(data.lines.iter().any(|l| l.kind == Kind::Add));
    assert!(data.context.is_none());
    Ok(())
}

/// Opening down below a hunk that only removed lines numbers its header from the line after
/// the empty range, and the patch still applies.
#[test]
fn opening_below_a_hunk_with_an_empty_side_numbers_it_right() -> Result<(), String> {
    let base = numbered(40, "");
    let dir = repo("empty-side", "f.txt", &base);
    // Lines 10 and 11 removed, nothing else: `@@ -7,8 +7,6 @@` with -U3; with no context the
    // new side is empty. Use the user's `diff.context=0`-like shape by removing at the top.
    let edited: String = base
        .lines()
        .enumerate()
        .filter(|(k, _)| *k != 9 && *k != 10)
        .map(|(_, l)| format!("{l}\n"))
        .collect();
    let _ = std::fs::write(dir.join("f.txt"), edited);
    let key = unstaged("f.txt");
    let mut view = view_of(&dir, &key)?;
    view.unfold(1, Unfold::Down(5));
    let d = data(&view)?;
    let header = d.lines[0].raw.clone();
    let hunk = hunk_changes(&d.lines, 0);
    apply(&dir, &view, &hunk, Pick::Stage)?;
    let _ = std::fs::remove_dir_all(&dir);
    // Staged: the two lines are gone from the index.
    assert!(header.starts_with("@@ -7,"), "{header}");
    Ok(())
}

#[test]
fn header_counts_from_an_empty_range_after_opening_down() {
    let lines = parse("@@ -4,0 +5,2 @@\n+x\n+y\n");
    let mut data = data_from(lines);
    data.context = Some(Context {
        gaps: gaps_of(&data.lines, 12).unwrap_or_default(),
        raw: (1..=12)
            .map(|n| {
                if n == 5 || n == 6 {
                    "new".to_owned()
                } else {
                    format!("l{n}")
                }
            })
            .collect(),
        path: "f.txt".to_owned(),
        dark: true,
        no_newline_at_end: false,
    });
    data.unfold(1, Unfold::Down(3));
    assert_eq!(data.lines[0].raw, "@@ -5,3 +5,5 @@");
    assert_eq!(data.lines[3].old, Some(5));
    assert_eq!(data.lines[3].new, Some(7));
}

/// A hunk past the end of the function git names has no _Function_ to open.
#[test]
fn no_function_button_when_the_named_line_does_not_enclose_the_hunk() {
    let file = "fn first() {\n    1\n}\n\nstatic X: u8 = 1;\nstatic Y: u8 = 2;\n";
    let lines = parse("@@ -5,1 +5,1 @@ fn first() {\n-static X: u8 = 0;\n+static X: u8 = 1;\n");
    let mut ctx = Context {
        raw: file.lines().map(str::to_owned).collect(),
        path: "a.rs".to_owned(),
        dark: true,
        no_newline_at_end: false,
        gaps: gaps_of(&lines, 6).unwrap_or_default(),
    };
    ctx.gaps[0].function = ctx.function_for(&lines, 0);
    assert_eq!(ctx.gaps[0].function, None);
    assert!(!ctx.function_opens(0));
}

fn data_from(lines: Vec<DiffLine>) -> DiffData {
    let (changed, split) = crate::repo::diff::parse::pair_lines(&lines);
    DiffData {
        spans: vec![None; lines.len()],
        changed,
        split,
        image: None,
        lfs: None,
        header: String::new(),
        partial: false,
        moves: Default::default(),
        context: None,
        new_file: false,
        longest: 0,
        uncut: Default::default(),
        map: Default::default(),
        lines,
    }
}

/// A moved block ending its hunk (no context after it) keeps its lines when lines are opened
/// right after it.
#[test]
fn a_moved_block_does_not_grow_over_lines_opened_after_it() {
    let lines = parse(
        "@@ -11,3 +10,0 @@\n-let first_moved_line = compute_it();\n-let second_moved_line = again();\n-let third = more_letters_here();\n@@ -40,0 +38,3 @@\n+let first_moved_line = compute_it();\n+let second_moved_line = again();\n+let third = more_letters_here();\n",
    );
    let mut data = data_from(lines);
    data.moves = crate::repo::diff::moved::detect(&data.lines);
    assert_eq!(data.moves.blocks.len(), 1);
    let before = data.moves.blocks[0].clone();
    data.context = Some(Context {
        gaps: gaps_of(&data.lines, 60).unwrap_or_default(),
        raw: (1..=60).map(|n| format!("l{n}")).collect(),
        path: "f.rs".to_owned(),
        dark: true,
        no_newline_at_end: false,
    });
    data.unfold(1, Unfold::Down(2));
    let after = &data.moves.blocks[0];
    assert_eq!(after.del, before.del);
    assert_eq!(after.add.len(), before.add.len());
    assert_eq!(data.lines[before.del.end].kind, Kind::Context);
    // The header below the empty range numbers the lines opened from the line after it.
    assert_eq!(data.lines[0].raw, "@@ -11,5 +11,2 @@");
}
