//! Owned-fixture gates. Compiled under the global runtime hold, never selected by the
//! `pure_deferred_` filter. Run only after the shared supervisor/helper gates are accepted.
use super::*;
use crate::git::drafts::virtuals::tests::Fixture;

#[test]
fn deferred_owned_conflict_keeps_source_index_files_and_branch_until_guarded_acceptance()
-> Result<(), String> {
    let f = Fixture::new()?;
    let base = f.git(&["rev-parse", "HEAD"])?;
    f.git(&["checkout", "-qb", "feature"])?;
    f.write("file", f.base().replace("line 3\n", "FEATURE\n").as_bytes())?;
    f.git(&["commit", "-qam", "feature"])?;
    let old = f.git(&["rev-parse", "HEAD"])?;
    f.git(&["checkout", "main"])?;
    f.write("file", f.base().replace("line 3\n", "MAIN\n").as_bytes())?;
    f.git(&["commit", "-qam", "main moved"])?;
    let onto = f.git(&["rev-parse", "HEAD"])?;
    f.git(&["checkout", "feature"])?;
    f.write("other", b"unrelated staged\n")?;
    f.git(&["add", "other"])?;
    f.write("other", b"unrelated working\n")?;
    f.write("untracked", b"unrelated untracked\n")?;
    let index = std::fs::read(f.path().join(".git/index")).map_err(|e| e.to_string())?;
    let original = std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?;
    let control = git::OpControl::new(|| {});
    let r = Runner {
        repo: f.path(),
        control: &control,
        credentials: None,
    };
    let active = || true;
    let before = read(f.path(), &active)?;
    start(
        &r,
        &before,
        "feature after main".into(),
        vec![Spec {
            expected: old.clone(),
            branch: "feature".into(),
            old_base: base,
            onto: onto.clone(),
            parent: "main".into(),
        }],
    )?;
    let v = read(f.path(), &active)?;
    let j = &v.jobs.jobs[0];
    let id = j.id.clone();
    let candidate = j.steps[0].worktree.clone();
    assert_eq!(j.steps[0].conflicts, ["file"]);
    assert_eq!(f.git(&["rev-parse", "feature"])?, old);
    assert_eq!(
        std::fs::read(f.path().join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?,
        original
    );
    assert_eq!(
        std::fs::read(f.path().join("other")).map_err(|e| e.to_string())?,
        b"unrelated working\n"
    );
    assert_eq!(
        std::fs::read(f.path().join("untracked")).map_err(|e| e.to_string())?,
        b"unrelated untracked\n"
    );
    std::fs::write(
        candidate.join("file"),
        f.base().replace("line 3\n", "MAIN plus FEATURE\n"),
    )
    .map_err(|e| e.to_string())?;
    git::run(Some(&candidate), &["add", "file"])?;
    resume(&r, &v, &id)?;
    let v = read(f.path(), &active)?;
    let result = v.jobs.jobs[0].steps[0]
        .result
        .clone()
        .ok_or("No candidate result")?;
    assert_ne!(result, old);
    assert_eq!(f.git(&["rev-parse", "feature"])?, old);
    assert_eq!(
        std::fs::read(f.path().join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    f.write("file", b"later original edit that must survive refusal\n")?;
    assert!(accept(&r, &v, &id, false).is_err());
    assert_eq!(f.git(&["rev-parse", "feature"])?, old);
    assert_eq!(
        std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?,
        b"later original edit that must survive refusal\n"
    );
    // Explicit fixture cleanup models the user's decision to save/clear original edits.
    f.git(&["restore", "--staged", "--worktree", "file", "other"])?;
    std::fs::remove_file(f.path().join("untracked")).map_err(|e| e.to_string())?;
    let v = read(f.path(), &active)?;
    reconcile(f.path(), &v, &id, &active)?;
    let v = read(f.path(), &active)?;
    accept(&r, &v, &id, false)?;
    assert_eq!(f.git(&["rev-parse", "feature"])?, result);
    let v = read(f.path(), &active)?;
    assert!(v.jobs.jobs[0].steps[0].accepted);
    close(f.path(), &v, &id, &active)?;
    let v = read(f.path(), &active)?;
    accept(&r, &v, &id, true)?;
    assert_eq!(f.git(&["rev-parse", "feature"])?, old);
    assert_eq!(
        std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?,
        original
    );
    assert!(
        candidate.exists(),
        "resolution evidence retained after undo"
    );
    Ok(())
}

#[test]
fn deferred_owned_stack_uses_parent_candidate_and_refuses_stale_source_tip() -> Result<(), String> {
    let f = Fixture::new()?;
    let base = f.git(&["rev-parse", "HEAD"])?;
    f.git(&["checkout", "-qb", "parent"])?;
    f.write("parent-file", b"parent\n")?;
    f.git(&["add", "parent-file"])?;
    f.git(&["commit", "-qm", "parent"])?;
    let parent = f.git(&["rev-parse", "HEAD"])?;
    f.git(&["checkout", "-qb", "child"])?;
    f.write("child-file", b"child\n")?;
    f.git(&["add", "child-file"])?;
    f.git(&["commit", "-qm", "child"])?;
    let child = f.git(&["rev-parse", "HEAD"])?;
    f.git(&["checkout", "main"])?;
    f.write("main-file", b"new main\n")?;
    f.git(&["add", "main-file"])?;
    f.git(&["commit", "-qm", "main moved"])?;
    let onto = f.git(&["rev-parse", "HEAD"])?;
    let control = git::OpControl::new(|| {});
    let r = Runner {
        repo: f.path(),
        control: &control,
        credentials: None,
    };
    let active = || true;
    let v = read(f.path(), &active)?;
    start(
        &r,
        &v,
        "two layers".into(),
        vec![
            Spec {
                expected: parent.clone(),
                branch: "parent".into(),
                old_base: base,
                onto: onto.clone(),
                parent: "main".into(),
            },
            Spec {
                expected: child.clone(),
                branch: "child".into(),
                old_base: parent.clone(),
                onto: parent.clone(),
                parent: "parent".into(),
            },
        ],
    )?;
    let v = read(f.path(), &active)?;
    let j = &v.jobs.jobs[0];
    assert_eq!(j.cursor, 2);
    assert_eq!(
        j.steps[1].onto,
        j.steps[0].result.clone().ok_or("expected Some")?
    );
    assert_eq!(f.git(&["rev-parse", "parent"])?, parent);
    assert_eq!(f.git(&["rev-parse", "child"])?, child);
    f.git(&["update-ref", "refs/heads/parent", &onto, &parent])?;
    assert!(accept(&r, &v, &j.id, false).is_err());
    assert_eq!(f.git(&["rev-parse", "parent"])?, onto);
    assert_eq!(f.git(&["rev-parse", "child"])?, child);
    assert_eq!(f.git(&["rev-parse", "HEAD"])?, onto);
    Ok(())
}
