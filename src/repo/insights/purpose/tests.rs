//! Review by purpose: the rules on synthetic units (a mixed pull request, a feature across
//! folders, the repository's own rules), the reviewer's edits and their revisions, and where a
//! change opens. Real repositories are in `tests/repo.rs`.

mod repo;

use super::arrange::{self, Edit, Placed, Saved};
use super::propose::{Proposal, propose};
use super::rows;
use super::rules::{self, Purpose};
use super::units::{Body, Markers, Unit, Units, Whole};

fn unit(path: &str, line: u32) -> Unit {
    Unit {
        key: format!("h:{path}:{line}"),
        path: path.to_owned(),
        old_path: None,
        status: 'M',
        body: Body::Hunk {
            old_start: line,
            old_len: 1,
            new_start: line,
            new_len: 1,
        },
        added: 1,
        removed: 1,
        preview: String::new(),
        markers: Markers::default(),
        repeated: None,
    }
}

fn whole(path: &str, why: Whole) -> Unit {
    Unit {
        key: format!("f:{path}"),
        body: Body::File(why),
        ..unit(path, 0)
    }
}

fn units(list: Vec<Unit>, head: &str) -> Units {
    Units {
        base: "base".into(),
        head: head.into(),
        units: list,
        ..Units::default()
    }
}

fn lockfiles(path: &str) -> Option<String> {
    path.ends_with(".lock").then(|| "lockfile".to_owned())
}

/// The titles of the groups, with the paths of their units.
fn titles(p: &Proposal, u: &Units) -> Vec<(String, Vec<String>)> {
    p.groups
        .iter()
        .map(|g| {
            let paths = g.units.iter().map(|&i| u.units[i].path.clone()).collect();
            (g.title.clone(), paths)
        })
        .collect()
}

/// A pull request touching an API, its logic, tests, configuration, docs, a lockfile, a moved
/// file and some formatting.
fn mixed() -> Units {
    let mut api = unit("src/lib.rs", 3);
    api.markers.api = Some("pub fn connect(timeout: u32)".into());
    let mut test = unit("src/net.rs", 90);
    test.markers.test = Some("#[test]".into());
    let mut moved = whole("src/old/name.rs", Whole::NoLines);
    moved.old_path = Some("src/name.rs".into());
    moved.status = 'R';
    let mut layout = unit("src/net.rs", 40);
    layout.markers.layout = true;
    units(
        vec![
            unit("Cargo.lock", 10),
            unit("Cargo.toml", 4),
            unit("README.md", 1),
            unit(".github/workflows/ci.yml", 7),
            api,
            unit("src/net.rs", 12),
            layout,
            test,
            moved,
            unit("proto/service.proto", 2),
            unit("tests/connect.rs", 1),
            whole("assets/logo.png", Whole::Binary),
            unit("migrations/0004_users.sql", 1),
        ],
        "head",
    )
}

#[test]
fn a_mixed_pull_request_is_grouped_by_purpose_with_every_change_once() {
    let u = mixed();
    let p = propose(&u, &lockfiles);
    let got = titles(&p, &u);
    let names: Vec<&str> = got.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(
        names,
        [
            "API and contracts",
            "Migrations and schema",
            "Runtime logic",
            "Tests",
            "Build and configuration",
            "Documentation",
            "Formatting only",
            "Moves and renames",
            "Images and other binaries",
            "Generated, vendored and lockfiles",
        ]
    );
    assert_eq!(got[0].1, ["src/lib.rs", "proto/service.proto"]);
    assert_eq!(got[3].1, ["src/net.rs", "tests/connect.rs"]);
    assert_eq!(got[4].1, ["Cargo.toml", ".github/workflows/ci.yml"]);
    assert_eq!(got[9].1, ["Cargo.lock"]);
    // Every placement says why.
    assert!(p.reasons[0].contains("lockfile"));
    assert!(p.reasons[4].contains("pub fn connect"));
    assert!(p.reasons[7].contains("#[test]"));
    assert!(p.reasons[8].contains("src/name.rs"));
    let a = arrange::arrange(&p, &u, None);
    assert_eq!(arrange::check(&a, &u), Ok(()));
    assert!(a.placed.iter().all(|p| matches!(p, Placed::Rule(_))));
}

#[test]
fn runtime_files_of_one_name_across_folders_are_one_feature_group() {
    let u = units(
        vec![
            unit("server/src/session.rs", 4),
            unit("src/util.rs", 1),
            unit("web/client/session.ts", 9),
            unit("web/client/session/index.ts", 2),
            unit("tests/session_tests.rs", 1),
        ],
        "head",
    );
    let p = propose(&u, &|_| None);
    let got = titles(&p, &u);
    assert_eq!(got[0].0, "Feature: session");
    assert_eq!(
        got[0].1,
        [
            "server/src/session.rs",
            "web/client/session.ts",
            "web/client/session/index.ts"
        ]
    );
    assert!(p.groups[0].why.contains("server/src") && p.groups[0].why.contains("web/client"));
    assert_eq!(got[1], ("Runtime logic".into(), vec!["src/util.rs".into()]));
    // Tests stay with the tests.
    assert_eq!(got[2].0, "Tests");
}

#[test]
fn the_repositorys_rules_come_first_and_lines_not_understood_are_said() {
    let mut u = units(
        vec![
            unit("src/billing/invoice.rs", 1),
            unit("e2e/login.spec.ts", 1),
            unit("web/billing/Invoice.tsx", 1),
            unit("src/main.rs", 1),
        ],
        "head",
    );
    u.config = Some(
        "# ours\ngroup Billing: src/billing/ web/billing/**\ntests: e2e/**\nbogus: x\nno colon\n"
            .into(),
    );
    let p = propose(&u, &|_| None);
    let got = titles(&p, &u);
    assert_eq!(
        got[0],
        (
            "Billing".into(),
            vec![
                "src/billing/invoice.rs".into(),
                "web/billing/Invoice.tsx".into()
            ]
        )
    );
    assert!(p.reasons[0].contains("line 2"));
    assert_eq!(p.errors.len(), 2, "{:?}", p.errors);
    assert!(
        got.iter()
            .any(|(t, paths)| t == "Tests" && paths == &["e2e/login.spec.ts"])
    );
}

#[test]
fn patterns_match_names_paths_and_folders() {
    assert!(rules::matches("*.proto", "a/b/c.proto"));
    assert!(rules::matches("src/*.rs", "src/a.rs"));
    assert!(!rules::matches("src/*.rs", "src/a/b.rs"));
    assert!(rules::matches("src/**/*.rs", "src/a/b.rs"));
    assert!(rules::matches("src/**/*.rs", "src/b.rs"));
    assert!(rules::matches("vendor/", "vendor/x/y.c"));
    assert!(rules::matches("/docs/?.md", "docs/a.md"));
    assert!(!rules::matches("docs/?.md", "docs/ab.md"));
    assert_eq!(Purpose::Api.title(), "API and contracts");
}

#[test]
fn renames_moves_splits_and_merges_are_kept_with_their_revisions() -> Result<(), String> {
    let u = mixed();
    let p = propose(&u, &lockfiles);
    let a = arrange::arrange(&p, &u, None);
    let tests_id = p.groups[3].id.clone();
    let docs_id = p.groups[5].id.clone();
    let saved = arrange::edit(
        &a,
        &u,
        Edit::Rename {
            group: tests_id.clone(),
            title: "  Proof  ".into(),
        },
    )?;
    assert_eq!((saved.base.as_str(), saved.head.as_str()), ("base", "head"));
    let a = arrange::arrange(&p, &u, Some(&saved));
    assert!(a.edited && a.stale.is_none());
    assert_eq!(a.groups[3].group.title, "Proof");
    // The README into the tests group, by hand.
    let readme = u.units[2].key.clone();
    let saved = arrange::edit(
        &a,
        &u,
        Edit::Move {
            units: vec![readme],
            to: tests_id.clone(),
        },
    )?;
    let a = arrange::arrange(&p, &u, Some(&saved));
    assert_eq!(a.placed[2], Placed::Yours);
    assert!(!a.groups.iter().any(|g| g.group.id == docs_id), "emptied");
    assert_eq!(arrange::check(&a, &u), Ok(()));
    // The two test units split off, shown right after.
    let keys = vec![u.units[7].key.clone(), u.units[10].key.clone()];
    let saved = arrange::edit(
        &a,
        &u,
        Edit::Split {
            units: keys,
            from: tests_id.clone(),
        },
    )?;
    let a = arrange::arrange(&p, &u, Some(&saved));
    assert_eq!(a.groups[4].group.units, [7, 10]);
    assert!(a.groups[4].yours);
    assert_eq!(a.groups[3].group.units, [2]);
    // Merged back, and the merged group goes up.
    let split = a.groups[4].group.id.clone();
    let saved = arrange::edit(
        &a,
        &u,
        Edit::Merge {
            from: split.clone(),
            into: tests_id.clone(),
        },
    )?;
    let a = arrange::arrange(&p, &u, Some(&saved));
    assert!(!a.groups.iter().any(|g| g.group.id == split));
    assert_eq!(a.groups[3].group.units, [2, 7, 10]);
    let saved = arrange::edit(
        &a,
        &u,
        Edit::Shift {
            group: tests_id.clone(),
            up: true,
        },
    )?;
    let a = arrange::arrange(&p, &u, Some(&saved));
    assert_eq!(a.groups[2].group.id, tests_id);
    let saved = arrange::edit(&a, &u, Edit::Confirm { group: tests_id })?;
    let a = arrange::arrange(&p, &u, Some(&saved));
    assert!([2, 7, 10].iter().all(|&i| a.placed[i] == Placed::Yours));
    assert!(matches!(a.placed[0], Placed::Rule(_)));
    assert_eq!(arrange::check(&a, &u), Ok(()));
    // Saved as text and read back the same.
    let text = serde_json::to_string(&saved).map_err(|e| e.to_string())?;
    let back: Saved = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back, saved);
    // A stale choice is refused, not guessed at.
    let gone = Edit::Move {
        units: vec!["h:nowhere:1".into()],
        to: "purpose:Docs".into(),
    };
    assert!(arrange::edit(&a, &u, gone).is_err());
    assert!(
        arrange::edit(
            &a,
            &u,
            Edit::Rename {
                group: "purpose:Logic".into(),
                title: " ".into()
            }
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn a_force_push_makes_an_arrangement_stale_until_it_is_carried_over() -> Result<(), String> {
    let before = mixed();
    let p = propose(&before, &lockfiles);
    let a = arrange::arrange(&p, &before, None);
    let moved = vec![before.units[1].key.clone(), before.units[2].key.clone()];
    let saved = arrange::edit(
        &a,
        &before,
        Edit::Move {
            units: moved,
            to: p.groups[0].id.clone(),
        },
    )?;
    // Force-pushed: a new head, and the README hunk is not the same any more.
    let mut after = mixed();
    after.head = "rewritten".into();
    after.units[2].key = "h:README.md:changed".into();
    let p2 = propose(&after, &lockfiles);
    let a2 = arrange::arrange(&p2, &after, Some(&saved));
    assert_eq!(a2.stale, Some(("base".into(), "head".into())));
    assert!(!a2.edited);
    assert!(a2.placed.iter().all(|p| matches!(p, Placed::Rule(_))));
    assert_eq!(a2.groups.len(), p2.groups.len());
    let (carried, dropped) = arrange::carry_over(&saved, &after);
    assert_eq!(dropped, 1);
    assert_eq!(carried.head, "rewritten");
    let a3 = arrange::arrange(&p2, &after, Some(&carried));
    assert!(a3.edited && a3.stale.is_none());
    assert_eq!(a3.placed[1], Placed::Yours);
    assert!(matches!(a3.placed[2], Placed::Rule(_)));
    assert_eq!(arrange::check(&a3, &after), Ok(()));
    Ok(())
}

#[test]
fn a_change_opens_in_the_pull_requests_own_diff_with_its_threads() {
    let files = crate::pull_request::PrFiles {
        from: "base".into(),
        to: "head".into(),
        files: Vec::new(),
    };
    let mut gone = unit("src/a.rs", 7);
    gone.body = Body::Hunk {
        old_start: 7,
        old_len: 2,
        new_start: 6,
        new_len: 0,
    };
    assert_eq!(rows::line_of(&gone), Some((true, 7)));
    assert_eq!(rows::line_of(&unit("src/a.rs", 3)), Some((false, 3)));
    assert_eq!(rows::line_of(&whole("x.png", Whole::Binary)), None);
    let key = rows::key_of(&files, &gone);
    assert_eq!(
        key,
        crate::repo::DiffKey::Range {
            from: "base".into(),
            to: Some("head".into()),
            path: "src/a.rs".into()
        }
    );
    let opened = rows::open(&files, &gone);
    assert!(matches!(&opened[0], crate::repo::Action::OpenDiff(k) if *k == key));
    let thread = |line, side, resolved| crate::pr_threads::Thread {
        id: "t".into(),
        path: "src/a.rs".into(),
        line: Some(line),
        start_line: None,
        side,
        outdated: false,
        resolved,
        can_resolve: false,
        can_unresolve: false,
        can_reply: false,
        comments: Vec::new(),
        starts_left: false,
        more_comments: false,
    };
    use crate::pull_request::DiffSide::{Left, Right};
    let threads = [
        thread(8, Left, false),
        thread(8, Right, false),
        thread(7, Left, true),
    ];
    assert_eq!(rows::threads_on(&threads, &gone), 1);
    // A hunk replacing line 8 has both sides' line 8.
    assert_eq!(rows::threads_on(&threads, &unit("src/a.rs", 8)), 2);
    assert_eq!(
        rows::threads_on(&threads, &whole("src/a.rs", Whole::TooLarge)),
        2
    );
}
