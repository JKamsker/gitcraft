//! Tests of moved-code detection: on diff text, and through the diff's loading on real
//! repositories.

use super::moved::{MovedBlock, compare, detect};
use super::*;
use crate::git;
use crate::repo::Action;

/// Lines of a function long enough to count as a move.
fn body(name: &str) -> String {
    format!(
        " fn {name}(total: usize) -> usize {{\n     let doubled = total * 2;\n     doubled + 1\n }}\n"
    )
}

/// `body` with every line marked `mark` (`+` or `-`).
fn marked(name: &str, mark: char) -> String {
    body(name)
        .lines()
        .map(|l| format!("{mark}{}\n", &l[1..]))
        .collect()
}

#[test]
fn a_block_removed_in_one_place_and_added_in_another_is_moved() {
    let text = format!(
        "@@ -1,8 +1,8 @@\n{}\n context one\n context two\n@@ -20,3 +20,7 @@\n tail\n{}",
        marked("alpha", '-'),
        marked("alpha", '+')
    );
    let lines = parse(&text);
    let moves = detect(&lines);
    assert_eq!(moves.blocks.len(), 1);
    let block = &moves.blocks[0];
    assert!(block.exact);
    assert_eq!(block.del.len(), 4);
    assert_eq!(block.add.len(), 4);
    assert!(lines[block.del.start].text.contains("fn alpha"));
    assert!(lines[block.add.start].text.contains("fn alpha"));
    for i in block.del.clone().chain(block.add.clone()) {
        assert_eq!(moves.at(i), Some((0, true)));
    }
    // Context lines are in no block.
    assert!(
        lines
            .iter()
            .enumerate()
            .all(|(i, l)| l.kind != Kind::Context || moves.at(i).is_none())
    );
}

#[test]
fn a_line_edited_in_place_or_re_indented_in_place_is_not_a_move() {
    // The same text removed and added in one change: re-indented, not moved.
    let text = "@@ -1,2 +1,2 @@\n-    let something_long_enough = compute_it();\n+let something_long_enough = compute_it();\n ctx\n";
    assert!(detect(&parse(text)).blocks.is_empty());
}

#[test]
fn short_blocks_of_braces_are_not_moves() {
    let text = "@@ -1,6 +1,6 @@\n-}\n-}\n ctx\n ctx\n+}\n+}\n";
    assert!(detect(&parse(text)).blocks.is_empty());
}

#[test]
fn a_block_moved_into_a_deeper_indentation_is_still_moved() {
    let deeper: String = body("beta")
        .lines()
        .map(|l| format!("+        {}\n", l[1..].trim()))
        .collect();
    let text = format!(
        "@@ -1,6 +1,2 @@\n{} a\n@@ -30,1 +26,5 @@\n b\n{deeper}",
        marked("beta", '-')
    );
    let moves = detect(&parse(&text));
    assert_eq!(moves.blocks.len(), 1);
    assert!(moves.blocks[0].exact);
}

#[test]
fn a_block_edited_on_the_way_is_one_block_with_its_edits_marked() {
    let old = "-fn gamma(total: usize) -> usize {\n-    let doubled = total * 2;\n-    let tripled = total * 3;\n-    doubled + tripled\n-}\n";
    let new = "+fn gamma(total: usize) -> usize {\n+    let doubled = total * 2;\n+    let tripled = total * 30;\n+    doubled + tripled\n+}\n";
    let text = format!("@@ -1,6 +1,1 @@\n{old} a\n@@ -40,1 +35,6 @@\n b\n{new}");
    let lines = parse(&text);
    let moves = detect(&lines);
    assert_eq!(moves.blocks.len(), 1, "{:?}", moves.blocks);
    let block = &moves.blocks[0];
    assert!(!block.exact);
    assert_eq!((block.del.len(), block.add.len()), (5, 5));
    let edited: Vec<usize> = block
        .del
        .clone()
        .chain(block.add.clone())
        .filter(|&i| moves.at(i) == Some((0, false)))
        .collect();
    assert_eq!(edited.len(), 2);
    assert!(
        edited
            .iter()
            .all(|&i| lines[i].text.contains("tripled = total"))
    );
    // The comparison pairs the kept lines and keeps the edited ones apart.
    let rows = compare(&lines, block);
    assert_eq!(
        rows.iter()
            .filter(|(o, n)| o.is_some() && n.is_some())
            .count(),
        4
    );
    assert_eq!(
        rows.iter()
            .filter(|(o, n)| o.is_none() || n.is_none())
            .count(),
        2
    );
}

#[test]
fn two_blocks_swapped_are_two_moves() {
    let text = format!(
        "@@ -1,9 +1,9 @@\n{}{} mid\n{}{}",
        marked("first", '-'),
        marked("second", '-'),
        marked("second", '+'),
        marked("first", '+')
    );
    let lines = parse(&text);
    let moves = detect(&lines);
    // One move each, never a block joined across the two functions.
    assert_eq!(moves.blocks.len(), 2);
    for MovedBlock { del, add, .. } in &moves.blocks {
        let name = |i: usize| lines[i].text.contains("first");
        assert_eq!(name(del.start), name(add.start));
    }
}

#[test]
fn unicode_lines_and_empty_or_combined_diffs() {
    let move_text = "-    let grüße = \"日本語のテキスト\" + \"ещё немного\";\n";
    let text = format!(
        "@@ -1,2 +1,1 @@\n{move_text} ctx\n@@ -9,1 +8,2 @@\n end\n+    let grüße = \"日本語のテキスト\" + \"ещё немного\";\n"
    );
    assert_eq!(detect(&parse(&text)).blocks.len(), 1);
    assert!(detect(&[]).blocks.is_empty());
    // A conflict's combined diff has no line numbers to tell its sides by.
    let combined = "@@@ -1,2 -1,2 +1,2 @@@\n- let something_long_enough = 1;\n ++let something_long_enough = 1;\n";
    assert!(find_moves(&parse(combined)).blocks.is_empty());
}

#[test]
fn many_repeated_lines_stay_fast_and_find_nothing() {
    let mut text = String::from("@@ -1,20000 +1,20000 @@\n");
    for _ in 0..10_000 {
        text.push_str("-    return something_quite_long;\n ctx\n");
    }
    for _ in 0..10_000 {
        text.push_str("+    return something_quite_long;\n x\n");
    }
    let started = std::time::Instant::now();
    let moves = detect(&parse(&text));
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    // The line is too common to start a block.
    assert!(moves.blocks.is_empty());
}

/// A repository in a fresh folder, a file with a space and accents in its name.
fn repo(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-moved-{name}-{}", std::process::id()));
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
    dir
}

const SOURCE: &str = "fn top() {}\n\nfn helper(total: usize) -> usize {\n    let doubled = total * 2;\n    doubled + 1\n}\n\nfn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\nfn e() {}\nfn f() {}\nfn g() {}\n";

/// `SOURCE` with `helper` moved to the end, its middle line `edit`ed.
fn moved_source(edit: &str) -> String {
    let helper =
        format!("fn helper(total: usize) -> usize {{\n    let doubled = total * 2;\n{edit}}}\n");
    let rest = SOURCE.replace(
        "fn helper(total: usize) -> usize {\n    let doubled = total * 2;\n    doubled + 1\n}\n\n",
        "",
    );
    format!("{rest}\n{helper}")
}

#[test]
fn moves_are_found_in_the_working_tree_and_in_commits_of_a_real_repository() -> Result<(), String> {
    let dir = repo("real");
    let name = "src dir/ünïcode file.rs";
    let _ = std::fs::create_dir_all(dir.join("src dir"));
    let file = dir.join(name);
    let _ = std::fs::write(&file, SOURCE);
    assert!(git::run(Some(&dir), &["add", "-A"]).is_ok());
    assert!(git::run(Some(&dir), &["commit", "-qm", "base"]).is_ok());

    // Moved unchanged, not committed yet.
    let _ = std::fs::write(&file, moved_source("    doubled + 1\n"));
    let key = DiffKey::Work(git::StatusFile {
        path: name.to_owned(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    });
    let data = load(&dir, &key, DiffOptions::default(), true);
    let blocks = data.as_ref().map(|d| d.moves.blocks.clone());
    // Lines can still be picked to stage: moves only change how they look.
    let partial = data.as_ref().map(|d| d.partial);

    // Moved and edited, committed.
    let _ = std::fs::write(&file, moved_source("    doubled + 100\n"));
    assert!(git::run(Some(&dir), &["commit", "-qam", "move"]).is_ok());
    let head = git::head(&dir);
    let commit = DiffKey::Commit {
        hash: head,
        path: name.to_owned(),
    };
    let edited = load(&dir, &commit, DiffOptions::default(), true);
    let whole = load(
        &dir,
        &commit,
        DiffOptions {
            whole_file: true,
            ..Default::default()
        },
        true,
    );
    let _ = std::fs::remove_dir_all(&dir);

    let blocks = blocks.unwrap_or_default();
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    assert!(blocks[0].exact);
    assert_eq!(blocks[0].del.len(), 4);
    assert_eq!(partial, Ok(true));
    for data in [edited, whole] {
        let data = data?;
        assert_eq!(data.moves.blocks.len(), 1, "{:?}", data.moves.blocks);
        let block = &data.moves.blocks[0];
        assert!(!block.exact);
        let changed: Vec<&str> = block
            .del
            .clone()
            .chain(block.add.clone())
            .filter(|&i| data.moves.at(i) == Some((0, false)))
            .map(|i| data.lines[i].text.as_str())
            .collect();
        assert_eq!(changed, ["    doubled + 1", "    doubled + 100"]);
    }
    Ok(())
}

/// The panel shows both places, and its buttons scroll the diff to either.
#[test]
fn the_moved_panel_names_both_places_and_goes_to_them() {
    use egui_kittest::kittest::Queryable;
    let text = format!(
        "@@ -1,8 +1,8 @@\n{}\n context one\n context two\n@@ -20,3 +20,7 @@\n tail\n{}",
        marked("alpha", '-'),
        marked("alpha", '+')
    );
    let data = data_of(&text);
    let block = data.moves.blocks[0].clone();
    let acts = std::cell::RefCell::new(Vec::new());
    let fonts = std::cell::Cell::new(false);
    let mut harness = egui_kittest::Harness::builder().build_ui(|ui| {
        // The theme's fonts are there from the frame after they are set.
        if !fonts.replace(true) {
            crate::theme::install_fonts(ui.ctx());
            return;
        }
        let t = crate::theme::Tokens::get(ui.ctx());
        if let Some(open) = super::moved_view::Opened::new(&data, 0) {
            super::moved_view::panel(ui, &t, &data, &open, &mut acts.borrow_mut());
        }
    });
    harness.run();
    assert!(
        harness
            .query_by_label_contains("Moved from lines 1–4 to lines")
            .is_some()
    );
    assert!(harness.query_by_label("unchanged on the way").is_some());
    harness.get_by_label("Go to where it was").click();
    harness.run();
    harness.get_by_label("Go to where it is now").click();
    harness.run();
    let acts = acts.borrow();
    assert!(
        acts.iter()
            .any(|a| matches!(a, Action::DiffJumpLine(i) if *i == block.del.start))
    );
    assert!(
        acts.iter()
            .any(|a| matches!(a, Action::DiffJumpLine(i) if *i == block.add.start))
    );
}

/// The diff `text` as the view gets it.
fn data_of(text: &str) -> DiffData {
    let lines = parse(text);
    let moves = detect(&lines);
    let (changed, split) = super::parse::pair_lines(&lines);
    DiffData {
        spans: vec![None; lines.len()],
        changed,
        split,
        image: None,
        lfs: None,
        header: String::new(),
        partial: false,
        moves,
        new_file: false,
        longest: 0,
        uncut: Default::default(),
        map: Default::default(),
        lines,
    }
}

/// One line changed on the way counts once, not as a removal and an addition.
#[test]
fn a_line_edited_on_the_way_counts_once() {
    let old = "-fn gamma(total: usize) -> usize {\n-    let doubled = total * 2;\n-    let tripled = total * 3;\n-    doubled + tripled\n-}\n";
    let new: String = old
        .lines()
        .map(|l| format!("+{}\n", &l[1..]))
        .collect::<String>()
        .replace("total * 3;", "total * 30;");
    let text = format!("@@ -1,6 +1,1 @@\n{old} a\n@@ -40,1 +35,6 @@\n b\n{new}");
    let data = data_of(&text);
    let open = super::moved_view::Opened::new(&data, 0);
    assert_eq!(open.map(|o| o.edited_lines()), Some(1));
}

/// A long block moved unchanged is compared line by line, however big.
#[test]
fn a_big_exact_move_pairs_every_line() {
    let block: String = (0..1500)
        .map(|n| format!("    let value_number_{n} = compute({n});\n"))
        .collect();
    let mark = |m: char| -> String { block.lines().map(|l| format!("{m}{l}\n")).collect() };
    let text = format!(
        "@@ -1,1501 +1,1 @@\n{} a\n@@ -3000,1 +1500,1501 @@\n b\n{}",
        mark('-'),
        mark('+')
    );
    let data = data_of(&text);
    assert_eq!(data.moves.blocks.len(), 1);
    assert!(data.moves.blocks[0].exact);
    let rows = compare(&data.lines, &data.moves.blocks[0]);
    assert_eq!(rows.len(), 1500);
    assert!(rows.iter().all(|(o, n)| o.is_some() && n.is_some()));
    let open = super::moved_view::Opened::new(&data, 0);
    assert_eq!(open.map(|o| o.edited_lines()), Some(0));
}

/// The panel stays open on the same block when the diff is read again, and a jump asked for
/// before is dropped rather than landing on another line.
#[test]
fn the_open_block_survives_a_reload() {
    let text = format!(
        "@@ -1,8 +1,8 @@\n{}\n context one\n context two\n@@ -20,3 +20,7 @@\n tail\n{}",
        marked("alpha", '-'),
        marked("alpha", '+')
    );
    let mut view = crate::repo::DiffView::new(DiffKey::Commit {
        hash: "x".to_owned(),
        path: "a.rs".to_owned(),
    });
    let request = view.next_request();
    view.arrived(request, Ok(data_of(&text)));
    if let Some(Ok(data)) = &view.data {
        view.moved = super::moved_view::Opened::new(data, 0);
    }
    view.jump_line = Some(3);
    let request = view.next_request();
    // Another change earlier in the file: the block keeps its place.
    let again = format!("@@ -1,1 +1,1 @@\n-x = 1\n+x = 2\n{text}");
    view.arrived(request, Ok(data_of(&again)));
    assert!(view.moved.is_some());
    assert_eq!(view.jump_line, None);
    // Gone from the diff: closed.
    let request = view.next_request();
    view.arrived(request, Ok(data_of("@@ -1,1 +1,1 @@\n-x = 1\n+x = 2\n")));
    assert!(view.moved.is_none());
}

/// A block joined across a few edited lines never takes in a line another block moved.
#[test]
fn blocks_never_share_a_line() {
    let text = "@@ -1,6 +1,6 @@\n-alpha_line_with_enough_letters_one\n-gap_del_unmatched_zzzzzzzzzzzzzzzz\n-beta_line_with_enough_letters_two\n ctx\n+alpha_line_with_enough_letters_one\n+gamma_line_with_enough_letters_three\n+beta_line_with_enough_letters_two\n ctx\n-gamma_line_with_enough_letters_three\n ctx\n";
    let lines = parse(text);
    let moves = detect(&lines);
    let mut seen = std::collections::HashSet::new();
    for block in &moves.blocks {
        for i in block.del.clone().chain(block.add.clone()) {
            assert!(seen.insert(i), "line {i} in two blocks: {:?}", moves.blocks);
        }
    }
    let gamma = lines
        .iter()
        .position(|l| l.kind == Kind::Add && l.text.starts_with("gamma"))
        .unwrap_or(0);
    let (b, same) = moves.at(gamma).unwrap_or((usize::MAX, false));
    assert!(same);
    assert!(moves.blocks[b].exact);
}
