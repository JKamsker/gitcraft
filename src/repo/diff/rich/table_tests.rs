//! Tests of tables (CSV, TSV) and of `.dvc` files.

use super::table::{self, Change};
use super::table_view::{Item, layout};
use super::*;

fn sides(old: &str, new: &str) -> Sides {
    Sides {
        old: Some(old.to_owned()),
        new: Some(new.to_owned()),
    }
}

#[test]
fn csv_cells_quotes_line_breaks_and_lines() -> Result<(), String> {
    let t = table::parse(
        "\u{feff}a,b\r\n\"x, y\",\"say \"\"hi\"\"\"\n\n\"multi\nline\",2\n3",
        ',',
    )?;
    assert_eq!(
        t.rows,
        vec![
            vec!["a", "b"],
            vec!["x, y", "say \"hi\""],
            vec!["multi\nline", "2"],
            vec!["3"],
        ]
    );
    assert_eq!(t.lines, [1, 2, 4, 6]);
    assert_eq!(
        table::parse("a,\"b\n", ','),
        Err("line 1: a quoted cell that never ends".to_owned())
    );
    assert_eq!(table::parse("", ',').map(|t| t.rows.len()), Ok(0));
    assert_eq!(
        table::parse("a\tb c\n", '\t').map(|t| t.rows),
        Ok(vec![vec!["a".to_owned(), "b c".to_owned()]])
    );
    Ok(())
}

#[test]
fn rows_and_cells_changed_added_and_removed() -> Result<(), String> {
    let old = "id,name,city\n1,Ann,Oslo\n2,Bob,Rome\n3,Cid,Lima\n";
    let new = "id,name,city\n1,Ann,Bergen\n3,Cid,Lima\n4,Dee,Kyiv\n";
    let d = table::diff("people.csv", &sides(old, new))?;
    assert!(d.keyed);
    let rows: Vec<(Change, Option<u32>, Option<u32>)> = d
        .rows
        .iter()
        .map(|r| (r.change, r.old_line, r.new_line))
        .collect();
    assert_eq!(
        rows,
        [
            (Change::Changed, Some(2), Some(2)),
            (Change::Removed, Some(3), None),
            (Change::Kept, Some(4), Some(3)),
            (Change::Added, None, Some(4)),
        ]
    );
    Ok(())
}

#[test]
fn renamed_columns_are_told_from_added_and_removed_ones() -> Result<(), String> {
    let old = "id,name,city,zip\n1,Ann,Oslo,0150\n2,Bob,Rome,00100\n";
    let new = "id,full name,town,country\n1,Ann,Oslo,NO\n2,Bob,Rome,IT\n";
    let d = table::diff("a.csv", &sides(old, new))?;
    let cols: Vec<(Change, Option<&str>, Option<&str>)> = d
        .columns
        .iter()
        .map(|c| {
            (
                c.change,
                c.old.as_ref().map(|x| x.1.as_str()),
                c.new.as_ref().map(|x| x.1.as_str()),
            )
        })
        .collect();
    assert_eq!(
        cols,
        [
            (Change::Kept, Some("id"), Some("id")),
            (Change::Changed, Some("name"), Some("full name")),
            (Change::Changed, Some("city"), Some("town")),
            (Change::Added, None, Some("country")),
            (Change::Removed, Some("zip"), None),
        ]
    );
    // The rows only differ in the columns added and removed.
    assert!(d.rows.iter().all(|r| r.change == Change::Changed));
    Ok(())
}

#[test]
fn rows_without_a_key_are_matched_by_content() -> Result<(), String> {
    let old = "a,b\nx,1\nx,1\ny,2\n";
    let new = "a,b\nnew,0\nx,1\nx,1\ny,3\n";
    let d = table::diff("a.csv", &sides(old, new))?;
    assert!(!d.keyed);
    let changes: Vec<Change> = d.rows.iter().map(|r| r.change).collect();
    assert_eq!(
        changes,
        [Change::Added, Change::Kept, Change::Kept, Change::Changed]
    );
    Ok(())
}

#[test]
fn unchanged_rows_fold_around_the_changes() {
    let r = |change| table::Row {
        change,
        old: None,
        new: None,
        old_line: None,
        new_line: None,
    };
    let mut rows: Vec<table::Row> = (0..10).map(|_| r(Change::Kept)).collect();
    rows.push(r(Change::Added));
    let items = layout(&rows, &Default::default());
    assert_eq!(
        items,
        [
            Item::Fold { first: 0, count: 9 },
            Item::Row(9),
            Item::Row(10)
        ]
    );
}

#[test]
fn malformed_and_big_tables() -> Result<(), String> {
    let d = table::diff("a.csv", &sides("a,b\n", "a,\"b\n"));
    assert!(d.is_err_and(|e| e.contains("CSV after the change isn't valid")));
    let make = |v: usize| -> String {
        let mut s = String::from("k,v\n");
        for n in 0..50_000 {
            s.push_str(&format!("{n},{}\n", if n == 777 { v } else { n }));
        }
        s
    };
    let started = std::time::Instant::now();
    let d = table::diff("big.csv", &sides(&make(1), &make(2)))?;
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(
        d.rows.iter().filter(|r| r.change != Change::Kept).count(),
        1
    );
    Ok(())
}

#[test]
fn dvc_files_show_the_data_they_track() -> Result<(), String> {
    let old = "outs:\n- md5: 3f2a9c\n  size: 1048576\n  hash: md5\n  path: data.csv\n";
    let new = "outs:\n- md5: 77aa01.dir\n  size: 2097152\n  nfiles: 12\n  hash: md5\n  path: data.csv\n- md5: 00ff\n  size: 10\n  path: model.bin\n";
    let d = dvc::diff(&sides(old, new))?;
    assert_eq!(d.pairs.len(), 2);
    let (o, n) = &d.pairs[0];
    assert_eq!(o.as_ref().and_then(|o| o.size), Some(1_048_576));
    assert_eq!(n.as_ref().and_then(|n| n.files), Some(12));
    assert!(d.pairs[1].0.is_none());
    assert_eq!(d.pairs[1].1.as_ref().map(|t| t.line), Some(7));
    assert!(dvc::diff(&sides(old, "foo: 1\n")).is_err_and(|e| e.contains("tracks nothing")));
    assert_eq!(RichKind::of("data/raw.csv.dvc"), [RichKind::Dvc]);
    assert_eq!(RichKind::of("A B/ñ.TSV"), [RichKind::Table]);
    Ok(())
}

/// Read from a real repository; the grid shows the rows and a click goes to a row's line.
#[test]
fn a_table_from_a_repository_and_the_grid() -> Result<(), String> {
    use crate::git;
    use egui_kittest::kittest::Queryable;
    let dir = std::env::temp_dir().join(format!("gitcraft-rich-table-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(dir.join("dätä"));
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    let name = "dätä/people list.csv";
    let _ = std::fs::write(dir.join(name), "id,name\n1,Ann\n2,Bob\n");
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-qm", "one"])?;
    let _ = std::fs::write(dir.join(name), "id,name\n1,Ann\n2,Bobby\n");
    let key = DiffKey::Work(git::StatusFile {
        path: name.to_owned(),
        old_path: None,
        code: 'M',
        area: git::Area::Unstaged,
    });
    let read = load(&dir, &key, RichKind::Table, None);
    let _ = std::fs::remove_dir_all(&dir);
    let Ok(Rich::Ready(diff)) = read else {
        return Err("not read as a table".to_owned());
    };
    let acts = std::cell::RefCell::new(Vec::new());
    let state = std::cell::RefCell::new(RichState::default());
    let fonts = std::cell::Cell::new(false);
    let mut harness = egui_kittest::Harness::builder()
        .with_size([800.0, 300.0])
        .build_ui(|ui| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            show(ui, &diff, &mut state.borrow_mut(), &mut acts.borrow_mut());
        });
    harness.run();
    harness.get_by_label("2, Bobby").click();
    harness.run();
    assert!(acts.borrow().iter().any(|a| matches!(
        a,
        Action::DiffGoToLine {
            old: None,
            new: Some(3)
        }
    )));
    Ok(())
}
