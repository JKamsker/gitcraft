use super::*;

#[test]
fn typed_preconditions_preserve_operands_and_each_validation_pass_order() {
    let argv = ops(&[
        &[EXPECT, "HEAD", "tip"],
        &[EXPECT_TREE, "HEAD", "tree"],
        &[EXPECT_SYMREF, "HEAD", "refs/heads/main", "/linked"],
        &[EXPECT_HTTPS_ROUTE, "opaque-route-proof"],
        &[EXPECT, "refs/stash", "stash-tip"],
        &["fetch"],
    ]);
    let plan = Plan::decode(&argv);
    assert_eq!(
        plan.refs().collect::<Vec<_>>(),
        [("HEAD", "tip"), ("refs/stash", "stash-tip")]
    );
    assert_eq!(plan.trees().collect::<Vec<_>>(), [("HEAD", "tree")]);
    assert_eq!(
        plan.symrefs().collect::<Vec<_>>(),
        [("HEAD", "refs/heads/main", Some("/linked"))]
    );
    assert_eq!(plan.routes().collect::<Vec<_>>(), ["opaque-route-proof"]);
    assert_eq!(
        plan.steps().map(|s| s.index).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5]
    );
}

#[test]
fn retries_keep_proofs_and_start_after_exact_completed_source_index() {
    let argv = ops(&[
        &[EXPECT, "HEAD", "tip"],
        &["remote", "add", "origin", "url"],
        &[TRY, "optional", "fetch"],
        &[EXPECT_HTTPS_ROUTE, "proof"],
        &["push", "origin"],
    ]);
    for stopped in [1, 2, 3, 4, 5] {
        let mut retry = argv.clone();
        retain_unfinished(&mut retry, stopped);
        let expected: Vec<_> = argv
            .iter()
            .enumerate()
            .filter(|(index, _)| *index >= stopped || [0, 3].contains(index))
            .map(|(_, step)| step.clone())
            .collect();
        assert_eq!(retry, expected);
    }
    assert!(
        matches!(Plan::decode(&argv).steps().nth(2).map(|step| &step.kind),
        Some(Step::Try { note: "optional", args }) if args == &["fetch"])
    );
}

#[test]
fn private_dispatch_steps_are_typed_without_reinterpreting_their_payloads() {
    let source = "gitcraft.commandSource=opaque-proof";
    let argv = ops(&[
        &[crate::push_preflight::APPROVED, "plan", "-c", source],
        &[crate::push_preflight::FIX, "plan", "fix"],
        &[crate::gerrit::AMEND, "amend", "-m", "message"],
        &[TRY],
        &[EXPECT, "HEAD"],
    ]);
    let plan = Plan::decode(&argv);
    let kinds: Vec<_> = plan.steps().map(|s| s.kind).collect();
    let evidence = ["-c".to_owned(), source.to_owned()];
    let amend = ["amend".to_owned(), "-m".to_owned(), "message".to_owned()];
    let malformed = |marker, count| Step::Malformed(Malformed::Operands { marker, count });
    assert_eq!(
        kinds,
        [
            Step::ReviewedPush {
                id: "plan",
                evidence: Some(&evidence)
            },
            Step::PreflightFix {
                id: "plan",
                fix: "fix"
            },
            Step::GerritAmend { args: &amend },
            malformed(TRY, 0),
            malformed(EXPECT, 1),
        ]
    );
    assert!(
        plan.steps()
            .zip(&argv)
            .all(|(step, original)| step.argv == original)
    );
}
