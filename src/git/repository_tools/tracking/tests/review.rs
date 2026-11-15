use super::*;

#[test]
fn fetched_ref_in_a_custom_destination_sets_its_actual_remote() -> Result<(), String> {
    let (repo, _hosts) = fixture("tracking-custom-destination")?;
    repo.git(&[
        "config",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/cache/*",
    ])?;
    // Duplicate refspecs of one remote do not create multiple owners.
    repo.git(&[
        "config",
        "--add",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/cache/*",
    ])?;
    repo.git(&["fetch", "origin"])?;
    let view = read(repo.path(), "main")?;
    assert!(!view.ambiguous_mappings);
    let choice = view
        .choices
        .iter()
        .find(|c| c.full == "refs/remotes/cache/topic")
        .ok_or("Missing custom-destination choice")?;
    assert_eq!(choice.remote, "origin");
    assert_eq!(choice.path, "origin/topic");
    assert_eq!(choice.source, "refs/heads/topic");
    let plan = view.plan(Some(&choice.full))?;
    run(&repo, &plan.ops)?;
    assert_eq!(values(repo.path(), "main")?.remote, ["origin"]);
    assert_eq!(values(repo.path(), "main")?.merge, ["refs/heads/topic"]);
    run(&repo, &plan.undo)?;
    assert!(!read(repo.path(), "main")?.has_tracking);
    Ok(())
}
#[test]
fn overlapping_remote_mappings_have_no_selectable_owner_in_either_config_order()
-> Result<(), String> {
    for order in [["origin", "alice"], ["alice", "origin"]] {
        let (repo, _hosts) = fixture(&format!("tracking-overlap-{}", order[0]))?;
        for remote in order {
            repo.git(&[
                "config",
                &format!("remote.{remote}.fetch"),
                "+refs/heads/*:refs/remotes/cache/*",
            ])?;
        }
        repo.git(&["fetch", "origin"])?;
        let view = read(repo.path(), "main")?;
        assert!(view.ambiguous_mappings);
        assert!(
            !view
                .choices
                .iter()
                .any(|c| c.full.starts_with("refs/remotes/cache/"))
        );
        assert!(view.plan(Some("refs/remotes/cache/topic")).is_err());
        assert!(!view.has_tracking);
        repo.git(&[
            "config",
            "remote.alice.fetch",
            "+refs/heads/*:refs/remotes/alice/*",
        ])?;
        let unique = read(repo.path(), "main")?;
        assert!(!unique.ambiguous_mappings);
        assert!(
            unique
                .choices
                .iter()
                .any(|c| c.full == "refs/remotes/cache/topic" && c.remote == "origin")
        );
    }
    Ok(())
}
#[test]
fn automatic_rebase_setup_never_changes_pull_behavior_during_tracking_edits() -> Result<(), String>
{
    let (repo, _hosts) = fixture("tracking-rebase-preserved")?;
    repo.git(&["config", "branch.autoSetupRebase", "always"])?;
    for existing in [None, Some("false")] {
        if let Some(value) = existing {
            repo.git(&["config", "branch.main.rebase", value])?;
        }
        let before = local_values(repo.path(), "branch.main.rebase")?;
        let plan = read(repo.path(), "main")?.plan(Some("refs/remotes/alice/topic"))?;
        for ops in [&plan.ops, &plan.undo, &plan.redo] {
            run(&repo, ops)?;
            assert_eq!(local_values(repo.path(), "branch.main.rebase")?, before);
        }
        let unset = read(repo.path(), "main")?.plan(None)?;
        for ops in [&unset.ops, &unset.undo, &unset.redo] {
            run(&repo, ops)?;
            assert_eq!(local_values(repo.path(), "branch.main.rebase")?, before);
        }
    }
    Ok(())
}
#[test]
fn literal_head_branch_is_rejected_before_git_and_never_changes_current_branch()
-> Result<(), String> {
    let (repo, _hosts) = fixture("tracking-literal-head")?;
    repo.git(&[
        "branch",
        "--set-upstream-to=refs/remotes/alice/topic",
        "--",
        "main",
    ])?;
    repo.git(&["update-ref", "refs/heads/HEAD", "refs/heads/main"])?;
    repo.git(&["config", "branch.HEAD.remote", "origin"])?;
    repo.git(&["config", "branch.HEAD.merge", "refs/heads/main"])?;
    let view = read(repo.path(), "HEAD")?;
    for target in [Some("refs/remotes/origin/topic"), None] {
        let plan = view.plan(target)?;
        let before = config(repo.path())?;
        assert_eq!(
            run(&repo, &plan.ops).err().as_deref(),
            Some(crate::i18n::text("tracking-reserved-head"))
        );
        assert_eq!(config(repo.path())?, before);
        // A nonexistent repository would fail check-ref-format: the guard runs first.
        assert_eq!(
            execute(&repo.path().join("absent"), &plan.ops[0][1], None)
                .err()
                .as_deref(),
            Some(crate::i18n::text("tracking-reserved-head"))
        );
    }
    Ok(())
}
