use super::*;
#[test]
fn radar_detects_equivalent_patch_then_revert_without_claiming_behavior() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("fix-radar")?;
    let base = r.tip("HEAD")?;
    let fix = r.commit("fix.txt", "fixed\n", "fix")?;
    let id = patch_id(r.path(), &fix, false)?;
    let reverse = patch_id(r.path(), &fix, true)?;
    r.branch("release", &base)?;
    r.git(&["cherry-pick", &fix])?;
    let tip = r.tip("HEAD")?;
    let (ids, _, complete) = history(r.path(), &tip)?;
    assert!(complete);
    assert!(matches!(
        status(&ids, &id, &reverse, None, r.path(), &tip),
        Patch::Same(_)
    ));
    r.git(&["revert", "--no-edit", &tip])?;
    let reverted = r.tip("HEAD")?;
    let (ids, _, _) = history(r.path(), &reverted)?;
    assert_eq!(
        status(&ids, &id, &reverse, None, r.path(), &reverted),
        Patch::Reverted(reverted.clone())
    );
    r.git(&["revert", "--no-edit", &reverted])?;
    let restored = r.tip("HEAD")?;
    let (ids, _, _) = history(r.path(), &restored)?;
    assert!(matches!(
        status(&ids, &id, &reverse, None, r.path(), &restored),
        Patch::Same(_)
    ));
    Ok(())
}
#[test]
fn r2_pure_adapted_reverts_reapplications_and_truncated_history() {
    let history = |ids: &[&str]| {
        ids.iter()
            .enumerate()
            .map(|(i, id)| ((*id).into(), format!("commit{i}")))
            .collect::<Vec<_>>()
    };
    let adaptation = Some(("adapted-oid", "adapted", "reverse-adapted"));
    assert_eq!(
        patch_status(&history(&["adapted"]), "fix", "reverse-fix", adaptation),
        Patch::Adapted("adapted-oid".into())
    );
    assert_eq!(
        patch_status(
            &history(&["reverse-adapted", "adapted", "fix"]),
            "fix",
            "reverse-fix",
            adaptation
        ),
        Patch::Reverted("commit0".into())
    );
    assert_eq!(
        patch_status(
            &history(&["adapted", "reverse-adapted", "fix"]),
            "fix",
            "reverse-fix",
            adaptation
        ),
        Patch::Adapted("adapted-oid".into())
    );
    assert_eq!(
        patch_status(&history(&["unrelated"]), "fix", "reverse-fix", adaptation),
        Patch::NotChecked
    );
    assert_eq!(
        patch_status(
            &history(&["fix", "reverse-adapted"]),
            "fix",
            "reverse-fix",
            adaptation
        ),
        Patch::Same("commit0".into())
    );
    // Patch classification cannot read or mutate independent behavior observations.
}
