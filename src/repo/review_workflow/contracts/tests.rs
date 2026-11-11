use super::*;
#[test]
fn rehearsal_separates_application_and_behavior_without_touching_checkout() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("contract-rehearsal")?;
    let base = r.tip("HEAD")?;
    let fix = r.commit("fix.txt", "fixed\n", "fix")?;
    r.git(&["branch", "release", &base])?;
    r.branch("conflict", &base)?;
    r.commit("fix.txt", "different\n", "conflict")?;
    r.checkout("main")?;
    std::fs::write(r.path().join("base.txt"), "keep my dirty edit\n").map_err(|e| e.to_string())?;
    let rows = run(
        r.path(),
        &Plan {
            fix: fix.clone(),
            targets: "release\nconflict".into(),
            command: "test -f fix.txt".into(),
            trusted: true,
        },
        &git::RunStop::default(),
    )?;
    let good = rows
        .iter()
        .find(|r| r.target == "release")
        .ok_or("missing release")?;
    assert!(good.applied.is_some());
    assert_eq!(good.test.as_ref().and_then(|t| t.code), Some(0));
    let conflict = rows
        .iter()
        .find(|r| r.target == "conflict")
        .ok_or("missing conflict")?;
    assert!(conflict.error.is_some());
    assert!(conflict.test.is_none());
    assert_eq!(r.tip("release")?, base);
    assert_eq!(r.tip("HEAD")?, fix);
    assert_eq!(
        std::fs::read_to_string(r.path().join("base.txt")).map_err(|e| e.to_string())?,
        "keep my dirty edit\n"
    );
    Ok(())
}
#[test]
fn contract_targets_resolve_immutable_tips_and_explicit_adaptations() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("contract-plan")?;
    let base = r.tip("HEAD")?;
    let fix = r.commit("fix.txt", "fixed\n", "fix")?;
    r.git(&["branch", "release", &base])?;
    let targets = targets(r.path(), &format!("release={fix}\nmain"))?;
    assert_eq!(targets.len(), 2);
    assert!(
        targets
            .iter()
            .any(|(n, t, a)| n == "release" && t == &base && a.as_ref() == Some(&fix))
    );
    assert!(self::targets(r.path(), "--help").is_err());
    assert!(self::targets(r.path(), "").is_err());
    let plan = Plan {
        fix,
        targets: "release".into(),
        command: "printf success".into(),
        trusted: false,
    };
    assert!(run(r.path(), &plan, &git::RunStop::default()).is_err());
    Ok(())
}
#[test]
fn r2_pure_backport_roots_are_content_addressed_and_include_applied_tree() {
    let first = evidence_roots("fix-A", "release-A", "adaptation-A", Some("composite-A"));
    let second = evidence_roots("fix-B", "release-B", "adaptation-B", Some("composite-B"));
    assert!(
        first.iter().any(
            |(reference, oid)| reference.ends_with("trees/composite-A") && oid == "composite-A"
        )
    );
    assert!(
        first
            .iter()
            .all(|(name, _)| !second.iter().any(|(other, _)| other == name))
    );
    assert_eq!(first.len(), 4);
}
#[test]
#[ignore = "R2 runtime hold: owned Git/GC fixture authored, not executed"]
fn r2_runtime_backport_roots_survive_gc_after_source_rewrites() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("backport-gc")?;
    let base = r.tip("HEAD")?;
    let fix = r.commit("fix.txt", "fix\n", "fix")?;
    r.git(&["reset", "--hard", &base])?;
    let target = r.commit("release-only.txt", "diverged\n", "release")?;
    r.git(&["cherry-pick", "--no-commit", &fix])?;
    let tree = git::run(Some(r.path()), &["write-tree"])?;
    retain_evidence(r.path(), &fix, &target, &fix, Some(tree.trim()))?;
    r.git(&["reset", "--hard", &base])?;
    let later = r.commit("later.txt", "later\n", "rewrite sources")?;
    retain_evidence(r.path(), &later, &base, &later, None)?;
    r.git(&["reflog", "expire", "--expire=now", "--all"])?;
    r.git(&["gc", "--prune=now"])?;
    r.git(&["cat-file", "-e", tree.trim()])?;
    r.git(&["cat-file", "-e", &fix])?;
    r.git(&["cat-file", "-e", &target])?;
    assert_eq!(
        r.git(&["show", &format!("{}:release-only.txt", tree.trim())])?,
        "diverged\n"
    );
    assert_eq!(
        r.git(&["show", &format!("{}:fix.txt", tree.trim())])?,
        "fix\n"
    );
    Ok(())
}
