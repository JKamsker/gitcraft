//! Regression coverage for the adversarial review findings in #225.
use super::super::units;
use super::*;
use crate::git::{self, fixture::Repo};

#[test]
fn star_patterns_are_bounded_and_keep_folder_semantics() {
    use super::super::glob;
    assert!(!glob::matches(
        &format!("{}z", "*a".repeat(90)),
        &"a".repeat(2000)
    ));
    for (pattern, path, yes) in [
        ("**/x.rs", "x.rs", true),
        ("**/x.rs", "a/b/x.rs", true),
        ("a/*.rs", "a/b/x.rs", false),
        ("a/", "a/b/x.rs", true),
        ("/x.rs", "a/x.rs", false),
    ] {
        assert_eq!(glob::matches(pattern, path), yes, "{pattern}: {path}");
    }
    let (_, errors) = rules::parse(&format!("tests: {}", "a".repeat(201)));
    assert!(!errors.is_empty());
}

#[test]
fn base_attributes_and_rules_size_are_used_despite_working_tree_edits() -> Result<(), String> {
    let repo = Repo::new("purpose-base-attrs")?;
    let base = repo.commit(
        "base",
        &[
            (".gitattributes", Some("x.rs linguist-generated\n")),
            ("x.rs", Some("old\n")),
            (units::CONFIG, Some(&"x".repeat(65537))),
        ],
    )?;
    let head = repo.commit(
        "head",
        &[
            ("x.rs", Some("new\n")),
            (".gitattributes", Some("x.rs linguist-generated=false\n")),
        ],
    )?;
    let files = git::diff_files(repo.path(), &base, Some(&head))?;
    let read = units::read(repo.path(), &base, &head, &files)?;
    assert!(read.config.is_none());
    assert!(read.notes.iter().any(|n| n.contains("64 KiB")));
    assert_eq!(
        read.attrs.as_ref().ok_or("base attributes missing")?["x.rs"],
        git::FoldAttr::Generated
    );
    let custom = crate::repo::folding::Rules::new(true, "custom/**");
    assert!(custom.reason("custom/data.bin", None).is_some());
    Ok(())
}

#[test]
fn unchanged_edits_in_changed_context_get_new_identity() -> Result<(), String> {
    let repo = Repo::new("purpose-context")?;
    let base = repo.commit("base", &[("x.txt", Some("before\nold\nafter\n"))])?;
    let head = repo.commit("head", &[("x.txt", Some("before\nnew\nafter\n"))])?;
    let files = git::diff_files(repo.path(), &base, Some(&head))?;
    let first = units::read(repo.path(), &base, &head, &files)?;
    let base2 = repo.commit("other base", &[("x.txt", Some("different\nold\nafter\n"))])?;
    let head2 = repo.commit("other head", &[("x.txt", Some("different\nnew\nafter\n"))])?;
    let files2 = git::diff_files(repo.path(), &base2, Some(&head2))?;
    let second = units::read(repo.path(), &base2, &head2, &files2)?;
    assert_ne!(first.units[0].key, second.units[0].key);
    Ok(())
}

#[test]
fn stray_diff_hunks_fail_accounting() {
    let mut read = crate::insights::transform::diff::Read::default();
    read.hunks.push(crate::insights::transform::diff::Hunk {
        path: "stray".into(),
        ..Default::default()
    });
    let mut transform = crate::insights::transform::analyze(
        crate::insights::transform::Subject {
            base: "b".into(),
            head: "h".into(),
            commit: None,
            scope: String::new(),
            tips: vec![],
        },
        read.clone(),
        &Default::default(),
    );
    transform.patterns.clear();
    assert!(units::build("b", "h", &[], &read, &transform).is_err());
}

#[test]
fn a_rename_across_path_chunk_boundaries_remains_one_unit() -> Result<(), String> {
    let repo = Repo::new("purpose-rename-chunks")?;
    let mut changes = vec![("000-old.rs".to_owned(), Some("old\n".to_owned()))];
    changes.extend((0..81).map(|i| (format!("middle-{i:03}.txt"), Some("before\n".to_owned()))));
    fn borrowed(items: &[(String, Option<String>)]) -> Vec<(&str, Option<&str>)> {
        items
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_deref()))
            .collect()
    }
    let base = repo.commit("base", &borrowed(&changes))?;
    let mut changes = vec![
        ("000-old.rs".to_owned(), None),
        ("zzz-new.rs".to_owned(), Some("old\nnew\n".to_owned())),
    ];
    changes.extend((0..81).map(|i| (format!("middle-{i:03}.txt"), Some("after\n".to_owned()))));
    let head = repo.commit("head", &borrowed(&changes))?;
    let files = git::diff_files(repo.path(), &base, Some(&head))?;
    let read = units::read(repo.path(), &base, &head, &files)?;
    assert_eq!(
        read.units.iter().filter(|u| u.path == "zzz-new.rs").count(),
        1
    );
    assert!(!read.units.iter().any(|u| u.path == "000-old.rs"));
    assert_eq!(
        arrange::check(
            &arrange::arrange(&propose(&read, &lockfiles), &read, None),
            &read
        ),
        Ok(())
    );
    Ok(())
}

#[test]
fn purpose_controls_and_group_names_are_translated() {
    for (language, order, title) in [
        ("en-US", "By purpose", "Runtime logic"),
        ("de", "Nach Zweck", "Programmlogik"),
    ] {
        crate::i18n::set_language(language);
        assert_eq!(crate::i18n::text("purpose-2"), order);
        assert_eq!(Purpose::Logic.title(), title);
        let message = crate::i18n::message("purpose-notice-dropped", &[("count", 2.into())]);
        assert!(!message.contains("purpose-notice"));
        assert!(message.contains('2'));
    }
    crate::i18n::set_language("en-US");
}
