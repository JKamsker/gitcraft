//! Teleports between two real repositories standing in for two machines: both here, and (on
//! Unix) one of them behind the loopback SSH host, whose shells are `sh` here, so that the
//! work goes through the very shells, framing and `base64 -d` another machine is reached by.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::*;
use crate::git;

include!("tests/fixtures.rs");

include!("tests/transfer.rs");

/// Stopped while it waits for a large bundle to be allowed: nothing is left on either side.
#[test]
fn a_large_bundle_waits_and_a_no_leaves_nothing() -> Result<(), String> {
    let _one = serial();
    let root = scratch("large")?;
    let (src, target) = source_with_work(&root)?;
    let before = Untouched::of(&src)?;
    let target_refs = git(&target, &["for-each-ref"])?;
    let mut plan = plan_to(&src, &target, &Machine::Here);
    plan.confirm_above = 0;
    let job = std::sync::Arc::new(job());
    let asked = std::sync::Arc::clone(&job);
    let answer = std::thread::spawn(move || {
        while !matches!(asked.step(), Step::Confirm(_)) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        asked.answer(false);
    });
    let result = run(&plan, &job);
    let _ = answer.join();
    let failed = result.err().ok_or("it landed")?;
    assert!(failed.cancelled);
    assert_eq!(failed.message, CANCELLED);
    assert!(!plan.folder.exists());
    assert_eq!(git(&target, &["for-each-ref"])?, target_refs);
    before.check()?;
    no_bundle_left();
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// What can't be carried is refused with its reason, before anything is made anywhere.
#[test]
fn what_cant_go_is_refused_with_a_reason() -> Result<(), String> {
    let _one = serial();
    let root = scratch("refused")?;
    let (src, target) = source_with_work(&root)?;
    let plan = plan_to(&src, &target, &Machine::Here);
    let refused = |plan: &Plan| {
        run(plan, &job())
            .err()
            .map(String::from)
            .unwrap_or_default()
    };

    // The branch is checked out there.
    git(&target, &["branch", "feature/x", "main"])?;
    git(&target, &["checkout", "-q", "feature/x"])?;
    let e = refused(&plan);
    assert!(e.contains("is checked out on this machine already"), "{e}");
    // There, with commits the source doesn't have.
    git(&target, &["checkout", "-q", "main"])?;
    git(&target, &["checkout", "-q", "feature/x"])?;
    git(
        &target,
        &["commit", "-q", "--allow-empty", "-m", "only there"],
    )?;
    git(&target, &["checkout", "-q", "main"])?;
    let e = refused(&plan);
    assert!(e.contains("has commits that aren't here"), "{e}");
    git(&target, &["branch", "-q", "-D", "feature/x"])?;
    // A folder that is taken.
    std::fs::create_dir_all(&plan.folder).map_err(|e| e.to_string())?;
    assert!(refused(&plan).contains("is already there"));
    std::fs::remove_dir_all(&plan.folder).map_err(|e| e.to_string())?;
    // A repository of its own inside the work.
    let nested = src.join("untracked dir/inner");
    std::fs::create_dir_all(&nested).map_err(|e| e.to_string())?;
    git(&nested, &["init", "-q"])?;
    configure(&nested)?;
    git(&nested, &["commit", "-q", "--allow-empty", "-m", "inner"])?;
    let before = Untouched::of(&src)?;
    let e = refused(&plan);
    assert!(e.contains("is a repository of its own"), "{e}");
    before.check()?;
    std::fs::remove_dir_all(&nested).map_err(|e| e.to_string())?;
    // A merge with conflicts.
    git(&src, &["reset", "-q", "--hard"])?;
    git(&src, &["clean", "-q", "-f", "-d"])?;
    git(&src, &["checkout", "-q", "-b", "other", "main"])?;
    write(&src.join("feature.txt"), b"other side\n")?;
    git(&src, &["add", "feature.txt"])?;
    git(&src, &["commit", "-qm", "other"])?;
    let _ = git(&src, &["merge", "-q", "feature/x"]);
    let e = refused(&plan);
    assert!(e.contains("merge is in progress"), "{e}");
    assert!(!plan.folder.exists());
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// Submodules aren't carried.
#[test]
fn submodules_are_refused() -> Result<(), String> {
    let _one = serial();
    let root = scratch("submodule")?;
    let (src, target) = source_with_work(&root)?;
    let sha = git(&src, &["rev-parse", "HEAD"])?;
    let entry = format!("160000,{},vendor/lib", sha.trim());
    git(&src, &["update-index", "--add", "--cacheinfo", &entry])?;
    let e = run(&plan_to(&src, &target, &Machine::Here), &job())
        .err()
        .map(String::from)
        .unwrap_or_default();
    assert!(e.contains("submodules"), "{e}");
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// A clone of the same project is found among the folders known on a machine, whatever its
/// remote's URL looks like.
#[test]
fn a_clone_of_the_project_is_found() -> Result<(), String> {
    let _one = serial();
    let root = scratch("find")?;
    let (src, target) = source_with_work(&root)?;
    let other = root.join("other");
    std::fs::create_dir_all(&other).map_err(|e| e.to_string())?;
    git(&other, &["init", "-q"])?;
    let urls: Vec<String> = git::remotes(&src)?.into_iter().map(|r| r.url).collect();
    let found = find_clones(
        &[
            other,
            target.join("dir with space"),
            root.join("nowhere"),
            target.clone(),
        ],
        &urls,
    );
    assert_eq!(found, vec![target]);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn pieces_cover_the_bundle_once() {
    assert_eq!(protocol::chunks(0, 4), vec![0..0]);
    assert_eq!(protocol::chunks(4, 4), vec![0..4]);
    assert_eq!(protocol::chunks(9, 4), vec![0..4, 4..8, 8..9]);
    assert_eq!(protocol::chunks(3, 0), vec![0..1, 1..2, 2..3]);
}

#[test]
fn the_bundle_is_the_snapshot_without_what_is_there() {
    let base = "a".repeat(40);
    assert_eq!(
        protocol::bundle_revs("refs/gitcraft/teleport/1", std::slice::from_ref(&base)),
        format!("refs/gitcraft/teleport/1\n^{base}\n")
    );
    assert_eq!(protocol::bundle_revs("r", &[]), "r\n");
}

#[test]
fn answers_are_read() {
    let (a, b, c) = ("a".repeat(40), "b".repeat(40), "c".repeat(64));
    // An annotated tag counts as its commit; a line that isn't a hash is dropped.
    let tips = protocol::parse_tips(&format!("{a} \n{b} {c}\n{a} \nnonsense\n"));
    assert_eq!(tips, vec![a.clone(), c.clone()]);
    let present = protocol::parse_present(&format!("{a} commit\n{b} missing\n{c} tag\n"));
    assert_eq!(present, vec![a.clone(), c]);
    let raw = format!(
        ":000000 160000 {z} {a} A\0vendor/x\0:100644 100644 {a} {b} M\0a b.txt\0",
        z = "0".repeat(40)
    );
    assert_eq!(protocol::parse_gitlinks(raw.as_bytes()), vec!["vendor/x"]);
    let attrs = b"big.psd\0filter\0lfs\0a.txt\0filter\0unspecified\0";
    assert_eq!(protocol::parse_lfs_paths(attrs), vec!["big.psd"]);
    assert_eq!(
        protocol::nul_list(&["a b".to_owned(), "\u{fc}".to_owned()]),
        "a b\0\u{fc}\0".as_bytes()
    );
}

#[test]
fn machines_are_listed_once_and_never_the_source_s() {
    use crate::remote::Host;
    let ssh = |h: &str| Machine::There(Host::Ssh(h.to_owned()));
    let known = vec![
        PathBuf::from("ssh://build/home/me/app"),
        PathBuf::from("ssh://other/srv/x"),
        PathBuf::from("/home/me/local"),
    ];
    let list = machines(
        Path::new("ssh://build/home/me/app"),
        &["Ubuntu".to_owned(), "-bad".to_owned()],
        &["build".to_owned(), "-oProxyCommand=x".to_owned()],
        &known,
    );
    assert_eq!(
        list,
        vec![
            Machine::Here,
            Machine::There(Host::Wsl("Ubuntu".to_owned())),
            ssh("other"),
        ]
    );
    assert!(!machines(Path::new("/home/me/app"), &[], &[], &[]).contains(&Machine::Here));
}

#[test]
fn a_worktree_goes_beside_its_repository() {
    assert_eq!(
        worktree_folder(Path::new("ssh://build/home/me/app"), Some("feat/x y"), ""),
        PathBuf::from("ssh://build/home/me/app-feat-x-y")
    );
    assert_eq!(
        worktree_folder(Path::new("/code/app"), None, "0123456789"),
        Path::new("/code").join("app-01234567")
    );
}

include!("tests/clone.rs");
