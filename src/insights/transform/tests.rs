//! Grouping edits: synthetic diffs (one migration made in many files, a few deviations, an
//! edit made once) and real repositories (a commit, a branch, a revision moving on).

use super::diff::{Hunk, Read};
use super::*;

fn hunk(path: &str, line: u32, removed: &[&str], added: &[&str]) -> Hunk {
    Hunk {
        path: path.to_owned(),
        old_start: line,
        old_len: removed.len() as u32,
        new_start: line,
        new_len: added.len() as u32,
        removed: removed.iter().map(|s| (*s).to_owned()).collect(),
        added: added.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn subject() -> Subject {
    Subject {
        base: "b".into(),
        head: "h".into(),
        commit: Some("h".into()),
        scope: "h against its first parent".into(),
        tips: vec![(SELECTED.into(), "h".into())],
    }
}

fn grouped(hunks: Vec<Hunk>) -> Transform {
    let read = Read {
        listed: hunks.len(),
        hunks,
        ..Read::default()
    };
    analyze(subject(), read, &Limits::default())
}

/// Twelve files making the same migration, two deviating, one edit like no other.
fn migration() -> Transform {
    let mut hunks: Vec<Hunk> = (0..12)
        .map(|i| {
            let (old, new) = (
                format!("    foo(arg{i});"),
                format!("    foo(arg{i}, ctx);"),
            );
            hunk(&format!("src/m{i:02}.rs"), 10 + i, &[&old], &[&new])
        })
        .collect();
    hunks.push(hunk("src/odd.rs", 4, &["foo(y);"], &["foo(y, None);"]));
    hunks.push(hunk(
        "src/m03.rs",
        40,
        &["foo(z, w);"],
        &["foo(z, w, ctx);"],
    ));
    hunks.push(hunk(
        "src/other.rs",
        7,
        &["let limit = 1;"],
        &["let limit = 2;"],
    ));
    grouped(hunks)
}

#[test]
fn the_same_migration_in_many_files_is_one_pattern_with_readable_placeholders() {
    let t = migration();
    let first = &t.patterns[0];
    assert_eq!(first.edits.len(), 12);
    assert_eq!(first.shape, Shape::Rewrite);
    assert_eq!(first.label(), "foo($a); → foo($a, ctx);");
    assert_eq!(first.per_file.len(), 12);
    assert_eq!(t.files.len(), 14);
    let m03 = t.files.iter().position(|f| f == "src/m03.rs").unwrap_or(99);
    assert_eq!(first.in_file(m03), 1);
}

#[test]
fn deviations_are_flagged_beside_their_pattern_and_never_merged_into_it() {
    let t = migration();
    let first = &t.patterns[0];
    assert_eq!(first.deviating, 2);
    // Each deviation keeps a row of its own, right after the pattern.
    for p in &t.patterns[1..3] {
        assert_eq!(p.deviates_from, Some(0));
        assert_eq!(p.edits.len(), 1);
    }
    let labels: Vec<String> = t.patterns[1..3].iter().map(|p| p.label()).collect();
    assert!(
        labels.contains(&"foo(y); → foo(y, None);".to_owned()),
        "{labels:?}"
    );
    assert!(
        labels.contains(&"foo(z, w); → foo(z, w, ctx);".to_owned()),
        "{labels:?}"
    );
    let m03 = t.files.iter().position(|f| f == "src/m03.rs").unwrap_or(99);
    assert_eq!(first.deviants_in(m03), 1);
    let (own, deviant) = t.cell(0, m03);
    assert_eq!((own.len(), deviant.len()), (1, 1));
    // The edit like no other stays visible, a pattern of one.
    let last = &t.patterns[3];
    assert_eq!(last.deviates_from, None);
    assert_eq!(last.label(), "let limit = 1; → let limit = 2;");
    let s = t.summary();
    assert_eq!(
        (s.edits, s.repeated, s.matching, s.deviating, s.one_off),
        (15, 1, 12, 2, 1)
    );
}

#[test]
fn additions_whitespace_and_large_edits_are_told_apart() {
    let big: Vec<String> = (0..300).map(|i| format!("x{i} = y{i} + 1;")).collect();
    let big: Vec<&str> = big.iter().map(String::as_str).collect();
    let t = grouped(vec![
        hunk("a.rs", 1, &[], &["use crate::ctx::Ctx;"]),
        hunk("b.rs", 1, &[], &["use crate::ctx::Ctx;"]),
        hunk("c.rs", 1, &[], &["use crate::ctx::Ctx;"]),
        hunk("d.rs", 1, &[], &["use crate::ctx::Context;"]),
        hunk("e.rs", 3, &["foo( a )"], &["foo(a)"]),
        hunk("f.rs", 1, &big, &["gone();"]),
    ]);
    assert_eq!(t.patterns[0].shape, Shape::Added);
    assert_eq!(t.patterns[0].label(), "+ use crate::ctx::Ctx;");
    assert_eq!(t.patterns[1].deviates_from, Some(0));
    let shapes: Vec<Shape> = t.patterns.iter().map(|p| p.shape).collect();
    assert!(shapes.contains(&Shape::Layout));
    assert!(shapes.contains(&Shape::Large));
}

#[test]
fn a_hunk_replacing_as_many_lines_as_it_removed_is_an_edit_per_line() {
    let t = grouped(vec![hunk(
        "a.rs",
        5,
        &["foo(a);", "foo(b);"],
        &["foo(a, ctx);", "foo(b, ctx);"],
    )]);
    assert_eq!(t.edits.len(), 2);
    assert_eq!(t.edits[1].lines(), (6, 6));
    assert_eq!(t.patterns.len(), 1);
    assert_eq!(t.patterns[0].edits.len(), 2);
    // A removal has the line it was at.
    let gone = grouped(vec![hunk("a.rs", 0, &["x"], &[])]);
    assert_eq!(gone.edits[0].lines(), (1, 1));
}

#[test]
fn nothing_read_is_nothing_grouped() {
    let t = grouped(Vec::new());
    assert!(t.patterns.is_empty());
    assert_eq!(t.summary(), Summary::default());
    assert_eq!(t.cell(3, 3), (Vec::new(), Vec::new()));
}

fn rust(calls: &[&str]) -> String {
    calls
        .iter()
        .map(|c| format!("fn f() {{\n    {c};\n}}\n"))
        .collect()
}

#[test]
fn a_real_commit_is_read_and_grouped_and_a_new_commit_reads_differently() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("insights-transform")?;
    let before = [
        ("a.rs", rust(&["foo(a)"])),
        ("b.rs", rust(&["foo(b)"])),
        ("c.rs", rust(&["foo(c)"])),
        ("d.rs", rust(&["foo(d)"])),
        ("logo.bin", "\0\x01".to_owned()),
    ];
    let files: Vec<(&str, Option<&str>)> =
        before.iter().map(|(p, t)| (*p, Some(t.as_str()))).collect();
    let root = repo.commit("one", &files)?;
    // The root commit compares with nothing: everything is added.
    let first = read(
        repo.path(),
        resolve(repo.path(), &Ask::Commit(root.clone()))?,
        &Limits::default(),
    )?;
    assert_eq!(first.binary, 1);
    assert!(first.patterns.iter().all(|p| p.shape == Shape::Added));
    let after = [
        ("a.rs", rust(&["foo(a, ctx)"])),
        ("b.rs", rust(&["foo(b, ctx)"])),
        ("c.rs", rust(&["foo(c, ctx)"])),
        ("d.rs", rust(&["foo(d, None)"])),
    ];
    let files: Vec<(&str, Option<&str>)> =
        after.iter().map(|(p, t)| (*p, Some(t.as_str()))).collect();
    let migrated = repo.commit("migrate", &files)?;
    let subject = resolve(repo.path(), &Ask::Commit("HEAD".into()))?;
    assert_eq!(subject.base, root);
    assert_eq!(subject.commit.as_deref(), Some(migrated.as_str()));
    let t = read(repo.path(), subject, &Limits::default())?;
    assert_eq!(t.patterns[0].label(), "foo($a); → foo($a, ctx);");
    assert_eq!(t.patterns[0].edits.len(), 3);
    assert_eq!(t.patterns[1].deviates_from, Some(0));
    assert_eq!(t.files, ["a.rs", "b.rs", "c.rs", "d.rs"]);
    let edit = &t.edits[t.patterns[0].edits[0]];
    assert_eq!(edit.lines(), (2, 2));
    // A commit later the selected commit has moved: what was read is stale, and the new one
    // reads differently.
    let mut p = t.provenance.clone();
    let next = repo.commit("more", &[("e.rs", Some("fn e() {}\n"))])?;
    p.check_tips(|_| Some(next.clone()));
    assert!(p.stale.is_some());
    let again = read(
        repo.path(),
        resolve(repo.path(), &Ask::Commit("HEAD".into()))?,
        &Limits::default(),
    )?;
    assert_ne!(again.patterns, t.patterns);
    assert_eq!(again.files, ["e.rs"]);
    // Limits make it partial.
    let few = read(
        repo.path(),
        resolve(repo.path(), &Ask::Commit(migrated))?,
        &Limits {
            max_files: 1,
            ..Limits::default()
        },
    )?;
    assert_eq!(few.files.len(), 1);
    assert!(few.provenance.partial.is_some());
    Ok(())
}

#[test]
fn a_branch_is_compared_since_its_merge_base_and_missing_heads_are_said() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("insights-transform-branch")?;
    let base = repo.commit("base", &[("a.rs", Some("foo(a);\n"))])?;
    // On the base itself, there is nothing of its own to compare.
    assert!(
        resolve(
            repo.path(),
            &Ask::Branch {
                base: "main".into()
            }
        )
        .is_err()
    );
    repo.git(&["checkout", "-q", "-b", "topic"])?;
    repo.commit("one", &[("a.rs", Some("foo(a, ctx);\n"))])?;
    repo.commit("two", &[("b.rs", Some("bar();\n"))])?;
    let subject = resolve(
        repo.path(),
        &Ask::Branch {
            base: "main".into(),
        },
    )?;
    assert_eq!(subject.base, base);
    assert_eq!(subject.commit, None);
    let t = read(repo.path(), subject, &Limits::default())?;
    assert_eq!(t.files, ["a.rs", "b.rs"]);
    let missing = Ask::Pr {
        number: 7,
        head: "0123456789012345678901234567890123456789".into(),
        bases: vec!["origin/main".into(), "main".into()],
    };
    let err = resolve(repo.path(), &missing).err().unwrap_or_default();
    assert!(err.contains("fetch"), "{err}");
    let gone = std::path::Path::new("/nonexistent/insights-transform");
    assert!(resolve(gone, &Ask::Commit("HEAD".into())).is_err());
    Ok(())
}
