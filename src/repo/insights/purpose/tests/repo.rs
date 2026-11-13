//! Review by purpose over a real repository: renames (alone and with edits), a binary, a
//! lockfile, a deletion, one edit repeated in three files, formatting, the repository's own
//! rules for a feature across folders, then a force push. Every hunk Git prints is one unit.

use super::super::{arrange, propose, store, units};
use super::{Edit, Placed};
use crate::git::{self, fixture::Repo};
use crate::repo::insights::purpose::units::{Body, Whole};

fn lines(n: usize, changed: Option<usize>) -> String {
    (0..n)
        .map(|i| match changed {
            Some(c) if c == i => format!("let v{i} = {i} + 1;\n"),
            _ => format!("let v{i} = {i};\n"),
        })
        .collect()
}

/// The base and the head of a pull request changing a bit of everything.
fn pull_request(repo: &Repo) -> Result<(String, String), String> {
    let (name, before, after) = (lines(12, None), lines(10, None), lines(10, Some(4)));
    let base = repo.commit(
        "base",
        &[
            ("src/name.rs", Some(name.as_str())),
            ("src/editme.rs", Some(before.as_str())),
            ("src/a.rs", Some("fn a() {\n    foo(alpha);\n}\n")),
            ("src/b.rs", Some("fn b() {\n    foo(beta);\n}\n")),
            ("src/c.rs", Some("fn c() {\n    foo(gamma);\n}\n")),
            ("src/fmt.rs", Some("fn f() {\n    let x = 1;\n}\n")),
            ("src/gone.rs", Some("fn gone() {}\n")),
            ("Cargo.lock", Some("version = 1\n")),
            ("README.md", Some("hello\n")),
            (
                ".gitcraft/review-groups",
                Some("group Billing: billing/ web/billing/\n"),
            ),
        ],
    )?;
    repo.git(&["checkout", "-q", "-b", "topic"])?;
    let head = repo.commit(
        "head",
        &[
            ("src/name.rs", None),
            ("src/moved/name.rs", Some(name.as_str())),
            ("src/editme.rs", None),
            ("src/core/editme.rs", Some(after.as_str())),
            ("src/a.rs", Some("fn a() {\n    foo(alpha, ctx);\n}\n")),
            ("src/b.rs", Some("fn b() {\n    foo(beta, ctx);\n}\n")),
            ("src/c.rs", Some("fn c() {\n    foo(gamma, ctx);\n}\n")),
            ("src/fmt.rs", Some("fn f() {\n    let  x  =  1;\n}\n")),
            ("src/gone.rs", None),
            ("Cargo.lock", Some("version = 2\n")),
            ("README.md", Some("hello, reviewer\n")),
            ("assets/logo.png", Some("PNG\0\0\u{1}binary")),
            ("tests/it.rs", Some("#[test]\nfn it() {}\n")),
            ("billing/pay.rs", Some("pub fn pay() {}\n")),
            ("web/billing/pay.ts", Some("export function pay() {}\n")),
        ],
    )?;
    Ok((base, head))
}

fn lockfiles(path: &str) -> Option<String> {
    crate::repo::folding::Rules::new(true, "")
        .reason(path, None)
        .map(|r| r.label().to_owned())
}

#[test]
fn every_hunk_and_file_of_a_real_pull_request_is_one_unit_in_one_group() -> Result<(), String> {
    let repo = Repo::new("review-purpose-accounting")?;
    let (base, head) = pull_request(&repo)?;
    let files = git::diff_files(repo.path(), &base, Some(&head))?;
    let u = units::read(repo.path(), &base, &head, &files)?;
    // Every file listed has a unit; every hunk Git prints is one.
    for f in &files {
        assert!(
            u.units.iter().any(|x| x.path == f.path),
            "{} missing",
            f.path
        );
    }
    let patch = repo.git(&["diff", "-U0", "-M", &base, &head])?;
    let printed = patch.lines().filter(|l| l.starts_with("@@ ")).count();
    let hunks = u
        .units
        .iter()
        .filter(|x| matches!(x.body, Body::Hunk { .. }))
        .count();
    assert_eq!(hunks, printed);
    let mut keys: Vec<&str> = u.units.iter().map(|x| x.key.as_str()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), u.units.len(), "keys are unique");
    // In the order the pull request lists its files.
    let order = |p: &str| files.iter().position(|f| f.path == p);
    assert!(
        u.units
            .windows(2)
            .all(|w| order(&w[0].path) <= order(&w[1].path))
    );
    let find = |p: &str| u.units.iter().find(|x| x.path == p).ok_or(p.to_owned());
    let moved = find("src/moved/name.rs")?;
    assert_eq!(moved.body, Body::File(Whole::NoLines));
    assert_eq!(moved.old_path.as_deref(), Some("src/name.rs"));
    let edited = find("src/core/editme.rs")?;
    assert_eq!(
        (edited.status, edited.old_path.as_deref()),
        ('R', Some("src/editme.rs"))
    );
    assert!(matches!(edited.body, Body::Hunk { new_start: 5, .. }));
    assert_eq!(find("assets/logo.png")?.body, Body::File(Whole::Binary));
    assert_eq!(find("src/gone.rs")?.status, 'D');
    assert!(find("src/fmt.rs")?.markers.layout);
    assert_eq!(u.repeated.len(), 1, "{:?}", u.repeated);
    assert_eq!(u.units.iter().filter(|x| x.repeated.is_some()).count(), 3);
    assert!(u.config.as_deref().is_some_and(|c| c.contains("Billing")));

    let p = propose::propose(&u, &lockfiles);
    let group = |title: &str| -> Vec<String> {
        p.groups
            .iter()
            .find(|g| g.title.starts_with(title))
            .map(|g| g.units.iter().map(|&i| u.units[i].path.clone()).collect())
            .unwrap_or_default()
    };
    assert_eq!(p.groups[0].title, "Billing");
    assert_eq!(group("Billing"), ["billing/pay.rs", "web/billing/pay.ts"]);
    assert_eq!(group("Repeated edit"), ["src/a.rs", "src/b.rs", "src/c.rs"]);
    assert_eq!(group("Moves"), ["src/moved/name.rs"]);
    assert_eq!(group("Generated"), ["Cargo.lock"]);
    assert_eq!(group("Images"), ["assets/logo.png"]);
    assert_eq!(group("Tests"), ["tests/it.rs"]);
    assert_eq!(group("Formatting"), ["src/fmt.rs"]);
    assert_eq!(group("Documentation"), ["README.md"]);
    let a = arrange::arrange(&p, &u, None);
    assert_eq!(arrange::check(&a, &u), Ok(()));
    let listed: usize = a.groups.iter().map(|g| g.group.units.len()).sum();
    assert_eq!(listed, u.units.len());
    Ok(())
}

#[test]
fn an_arrangement_is_saved_locally_and_a_force_push_leaves_it_stale() -> Result<(), String> {
    let repo = Repo::new("review-purpose-force-push")?;
    let (base, head) = pull_request(&repo)?;
    let files = git::diff_files(repo.path(), &base, Some(&head))?;
    let u = units::read(repo.path(), &base, &head, &files)?;
    let p = propose::propose(&u, &lockfiles);
    let a = arrange::arrange(&p, &u, None);
    let test = u
        .units
        .iter()
        .position(|x| x.path == "tests/it.rs")
        .ok_or("no test unit")?;
    let saved = arrange::edit(
        &a,
        &u,
        Edit::Move {
            units: vec![u.units[test].key.clone()],
            to: "named:Billing".into(),
        },
    )?;
    let url = "https://example.com/o/r/pull/7";
    let branches = repo.git(&["for-each-ref", "refs/heads"])?;
    store::save(repo.path(), url, Some(&saved))?;
    assert_eq!(store::load(repo.path(), url)?, Some(saved.clone()));
    assert_eq!(repo.git(&["for-each-ref", "refs/heads"])?, branches);
    assert_eq!(
        store::load(repo.path(), "https://example.com/o/r/pull/8")?,
        None
    );

    // The author rewrites the head: the README changes again, the rest stays as it was.
    std::fs::write(repo.path().join("README.md"), "hello, again\n").map_err(|e| e.to_string())?;
    repo.git(&["commit", "-q", "-a", "--amend", "-m", "head, rewritten"])?;
    let pushed = repo.rev("HEAD")?;
    assert_ne!(pushed, head);
    let files = git::diff_files(repo.path(), &base, Some(&pushed))?;
    let u2 = units::read(repo.path(), &base, &pushed, &files)?;
    let p2 = propose::propose(&u2, &lockfiles);
    let loaded = store::load(repo.path(), url)?;
    let stale = arrange::arrange(&p2, &u2, loaded.as_ref());
    assert_eq!(stale.stale, Some((base.clone(), head.clone())));
    assert!(stale.placed.iter().all(|p| matches!(p, Placed::Rule(_))));
    let (carried, dropped) = arrange::carry_over(&saved, &u2);
    assert_eq!(dropped, 0, "the test file is the same");
    let a2 = arrange::arrange(&p2, &u2, Some(&carried));
    let test2 = u2
        .units
        .iter()
        .position(|x| x.path == "tests/it.rs")
        .ok_or("no test unit")?;
    assert_eq!(a2.placed[test2], Placed::Yours);
    assert!(a2.groups[0].group.units.contains(&test2));
    assert_eq!(arrange::check(&a2, &u2), Ok(()));
    store::save(repo.path(), url, None)?;
    assert_eq!(store::load(repo.path(), url)?, None);
    Ok(())
}
