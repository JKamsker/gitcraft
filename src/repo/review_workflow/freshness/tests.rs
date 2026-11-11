use super::*;
#[test]
fn exact_receipts_distinguish_direct_dependency_and_unknown() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("freshness")?;
    r.commit("caller.rs", "callee()\n", "caller")?;
    let old = r.commit("callee.rs", "ok\n", "callee")?;
    let base = r.tip("HEAD~2")?;
    let a = fingerprint(r.path(), &base, &old, "caller.rs", &["callee.rs".into()])?;
    let head = r.commit("callee.rs", "err\n", "behavior changed")?;
    let b = fingerprint(r.path(), &base, &head, "caller.rs", &["callee.rs".into()])?;
    assert_eq!(
        classify(&a, &b),
        Status::Dependency(vec!["callee.rs".into()])
    );
    let head = r.commit("caller.rs", "callee()?\n", "caller changed")?;
    assert_eq!(
        classify(
            &a,
            &fingerprint(r.path(), &base, &head, "caller.rs", &["callee.rs".into()])?
        ),
        Status::Direct
    );
    assert_eq!(classify(&a, &a), Status::Unchanged);
    Ok(())
}
