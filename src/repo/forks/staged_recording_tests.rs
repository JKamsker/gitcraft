use super::*;

fn plan() -> plan::Rebase {
    plan::Rebase {
        local: "topic".into(),
        fork: "fork".into(),
        branch: "contributor".into(),
        onto: "refs/remotes/origin/main".into(),
        head: "a".repeat(40),
        seen: "b".repeat(40),
        tracking: "refs/remotes/fork/contributor".into(),
        behind: 1,
        destinations: vec![guard::Destination {
            name: "fork".into(),
            fetch: vec!["https://forge.invalid/person/repo".into()],
            push: vec!["https://forge.invalid/person/repo".into()],
        }],
        base_head: "c".repeat(40),
    }
}

/// Supplied operation state and snapshots only: no RepoTab, Git or runtime.
#[test]
fn recording_local_rewrite_precedes_exact_tip_review_and_has_separate_undo() -> Result<(), String> {
    let p = plan();
    let local = p.local_steps();
    assert!(
        crate::push_preflight::validate_operations(&p.steps()).is_err(),
        "the mixed-operation scanner restriction remains intact"
    );
    assert!(!crate::push_preflight::has_push(&local));
    assert!(local.iter().any(|step| guard::is_guard(step)));
    assert_eq!(
        local.last(),
        Some(&vec![
            "-c".into(),
            "rebase.updateRefs=false".into(),
            "rebase".into(),
            p.base_head.clone()
        ])
    );
    let rewrite = guard::Rewrite {
        branch: "refs/heads/topic".into(),
        before: p.head.clone(),
        tip: "d".repeat(40),
        conflicts: false,
    };
    let label = "Rebase #7 onto main and push";
    let mut forks = Forks {
        running: Some((label.into(), p.clone())),
        staged_rebase: Some(label.into()),
        ..Forks::default()
    };
    assert!(forks.take_rebase_completion("unrelated").is_none());
    let (staged, _, taken) = forks.take_rebase_completion(label).ok_or("lost stage")?;
    assert!(staged);
    assert!(
        forks.take_rebase_completion(label).is_none(),
        "completion/undo must be consumed once"
    );
    let tip = taken
        .reviewed_tip(&rewrite, true, true)
        .ok_or("valid rewrite refused")?;
    let (undo, redo) = taken.local_undo(tip);
    assert!(!crate::push_preflight::has_push(&undo));
    assert!(!crate::push_preflight::has_push(&redo));
    assert!(
        undo.iter()
            .any(|step| step == &super::super::operations::expect_ref_at("refs/heads/topic", tip))
    );
    let pending = taken.push_pending(tip);
    crate::push_preflight::validate_operations(&pending.ops)?;
    assert!(crate::push_preflight::has_push(&pending.ops));
    assert!(
        !pending
            .ops
            .iter()
            .flatten()
            .any(|word| word == "rebase" || word == "reset")
    );
    assert!(
        pending
            .ops
            .iter()
            .any(|step| step == &super::super::operations::expect_ref_at("refs/heads/topic", tip))
    );
    let push = pending.ops.last().ok_or("missing push")?;
    assert!(push.contains(&format!(
        "--force-with-lease=refs/heads/contributor:{}",
        p.seen
    )));
    assert_eq!(push.last(), Some(&format!("{tip}:refs/heads/contributor")));
    let OpKind::Normal(Undo::Custom { undo, redo }, _) = pending.kind else {
        return Err("missing fork undo".into());
    };
    assert!(
        undo.iter()
            .flatten()
            .any(|word| word.contains(guard::PUSHED_TIP))
    );
    assert!(!undo.iter().flatten().any(|word| word == "reset"));
    assert_eq!(redo, pending.ops);
    Ok(())
}

#[test]
fn recording_error_cancel_conflict_stale_or_unowned_rewrite_never_enters_review() {
    let p = plan();
    let rewrite = guard::Rewrite {
        branch: "refs/heads/topic".into(),
        before: p.head.clone(),
        tip: "d".repeat(40),
        conflicts: false,
    };
    assert!(p.reviewed_tip(&rewrite, false, true).is_none()); // includes failed and cancelled OpDone
    assert!(p.reviewed_tip(&rewrite, true, false).is_none());
    for change in 0..5 {
        let mut invalid = rewrite.clone();
        match change {
            0 => invalid.conflicts = true,
            1 => invalid.branch = "refs/heads/other".into(),
            2 => invalid.before = "e".repeat(40),
            3 => invalid.tip = p.head.clone(),
            _ => invalid.tip.clear(),
        }
        assert!(p.reviewed_tip(&invalid, true, true).is_none());
    }
}
