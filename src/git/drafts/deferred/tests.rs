use super::*;
pub(crate) fn view() -> View {
    let common = PathBuf::from("/private/synthetic/project/.git");
    let oid = "a".repeat(40);
    View {
        jobs: Jobs {
            jobs: vec![Job {
                id: "synthetic-proposal".into(),
                title: "feature after main".into(),
                cursor: 0,
                busy: false,
                cancelled: false,
                notice: "Conflict waiting in isolated candidate".into(),
                steps: vec![Step {
                    branch: "feature".into(),
                    old: oid.clone(),
                    old_base: "b".repeat(40),
                    parent: "main".into(),
                    onto: "c".repeat(40),
                    worktree: common.join("gitcraft/deferred-rewrites/synthetic-proposal/step-0"),
                    made: true,
                    root_identity: Some("1:32".into()),
                    result: None,
                    accepted: false,
                    conflicts: vec!["src/shared.rs".into()],
                }],
            }],
        },
        version: "d".repeat(40),
        branch: "feature".into(),
        branches: vec![("feature".into(), oid)],
        common,
    }
}
#[test]
fn pure_deferred_ownership_requires_exact_repository_registration_and_backlink()
-> Result<(), String> {
    let v = view();
    let s = &v.jobs.jobs[0].steps[0];
    let gd = v.common.join("worktrees/step-0");
    let gitfile = format!("gitdir: {}\n", gd.display());
    let backlink = format!("{}\n", s.worktree.join(".git").display());
    assert_eq!(
        ownership_paths(
            &v.common,
            &s.worktree,
            gitfile.as_bytes(),
            backlink.as_bytes()
        )
        .map_err(|error| error.to_string())?,
        gd
    );
    for foreign in [
        "gitdir: /other/.git/worktrees/step-0\n",
        "gitdir: /private/synthetic/project/.git/worktrees/../other\n",
        "gitdir: /private/synthetic/project/.git/worktrees/step-0/other\n",
        "ref: refs/heads/main\n",
    ] {
        assert!(
            ownership_paths(
                &v.common,
                &s.worktree,
                foreign.as_bytes(),
                backlink.as_bytes()
            )
            .is_err()
        );
    }
    assert!(ownership_paths(&v.common, &s.worktree, gitfile.as_bytes(), b"/other/.git\n").is_err());
    Ok(())
}
#[test]
fn pure_deferred_metadata_refuses_unowned_paths_receipt_gaps_duplicate_layers_and_forward_parents()
{
    let original = view();
    assert!(validate(&original).is_ok());
    let mut v = original.clone();
    v.jobs.jobs[0].steps[0].worktree = "/unrelated".into();
    assert!(validate(&v).is_err());
    let mut v = original.clone();
    v.jobs.jobs[0].steps[0].root_identity = None;
    assert!(validate(&v).is_err());
    let mut v = original.clone();
    v.jobs.jobs[0].cursor = 1;
    assert!(validate(&v).is_err());
    let mut v = original.clone();
    v.jobs.jobs[0].steps[0].accepted = true;
    assert!(validate(&v).is_err());
    let mut v = original.clone();
    v.jobs.jobs.push(v.jobs.jobs[0].clone());
    assert!(validate(&v).is_err());
    let mut v = original.clone();
    let mut step = v.jobs.jobs[0].steps[0].clone();
    step.worktree = v
        .common
        .join("gitcraft/deferred-rewrites/synthetic-proposal/step-1");
    v.jobs.jobs[0].steps.push(step);
    assert!(validate(&v).is_err());
    let mut v = original.clone();
    v.jobs.jobs[0].steps[0].parent = "feature".into();
    assert!(validate(&v).is_err());
    let mut v = original.clone();
    v.jobs.jobs[0].steps[0].branch = " feature ".into();
    assert!(validate(&v).is_err());
    let mut v = original;
    let mut step = v.jobs.jobs[0].steps[0].clone();
    step.branch = "main".into();
    step.parent = "upstream".into();
    step.worktree = v
        .common
        .join("gitcraft/deferred-rewrites/synthetic-proposal/step-1");
    v.jobs.jobs[0].steps.push(step);
    assert!(validate(&v).is_err());
}
#[test]
fn pure_deferred_replay_never_resets_resolutions_and_uncertain_acceptance_requires_inspection() {
    assert_eq!(replay(true, true), Replay::Continue);
    assert_eq!(replay(true, false), Replay::Continue);
    assert_eq!(replay(false, true), Replay::Start);
    assert_eq!(replay(false, false), Replay::Reconcile);
    let mut j = view().jobs.jobs.remove(0);
    assert!(acceptance_allowed(&j, false).is_err());
    j.cursor = 1;
    j.steps[0].result = Some("e".repeat(40));
    assert!(acceptance_allowed(&j, false).is_ok());
    j.busy = true;
    assert!(acceptance_allowed(&j, false).is_err());
    assert!(acceptance_allowed(&j, true).is_err());
    j.busy = false;
    j.cancelled = true;
    j.steps[0].accepted = true;
    assert!(acceptance_allowed(&j, false).is_err());
    assert!(acceptance_allowed(&j, true).is_ok());
}
#[test]
fn pure_deferred_recording_enters_verified_cwd_before_git_and_keeps_paths_as_argv()
-> Result<(), String> {
    let mut v = view();
    let s = &mut v.jobs.jobs[0].steps[0];
    s.worktree = PathBuf::from("/private/synthetic/space ; $(not-executed)");
    let metadata = Path::new("/private/synthetic/.git/worktrees/step-0");
    let binding = DispatchBinding {
        metadata: metadata.into(),
        metadata_identity: "2:34".into(),
        common: v.common.clone(),
        common_identity: "2:30".into(),
        link_identity: "2:35".into(),
        link_text: format!("gitdir: {}", metadata.display()),
    };
    let result = owned_command(s, &binding, &binding, &["rebase", "--continue"]);
    if cfg!(windows) {
        let error = result.err().ok_or("Windows replay must refuse dispatch")?;
        assert!(
            error.contains("requires native Unix descriptor ownership"),
            "{error}"
        );
        return Ok(());
    }
    let command = result?;
    let shared = owned_command_at(
        &s.worktree,
        s.root_identity.as_deref(),
        &binding,
        &binding,
        &["rebase", "--continue"],
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(
        command, shared,
        "concrete subtree adapter preserves the existing command contract"
    );
    assert_eq!(command[3], "1:32");
    assert_eq!(command[4], s.worktree.to_str().ok_or("expected Some")?);
    assert_eq!(&command[11..], ["rebase", "--continue"]);
    assert!(!command[1].contains("not-executed"));
    // Recording only: substitute the second observed link/registration without
    // touching a filesystem or starting Git. No mutation command is produced.
    for field in 0..6 {
        let mut changed = binding.clone();
        match field {
            0 => changed.link_text = "gitdir: /foreign/.git".into(),
            1 => changed.link_identity = "9:91".into(),
            2 => changed.metadata = "/foreign/.git".into(),
            3 => changed.metadata_identity = "9:92".into(),
            4 => changed.common_identity = "9:93".into(),
            _ => changed.common = "/foreign/common".into(),
        }
        let mut dispatched = Vec::new();
        if let Ok(words) = owned_command(s, &binding, &changed, &["rebase", "--continue"]) {
            dispatched.push(words);
        }
        assert!(dispatched.is_empty());
    }
    let dispatch = OWNED_ALIAS
        .rfind("GIT_COMMON_DIR=")
        .ok_or("expected dispatch")?;
    for guard in [
        "token .",
        "token_fd <&6",
        "cat <&6",
        "token_fd <&7",
        "token_fd <&8",
        r#"token "$metadata""#,
        r#"token "$common""#,
    ] {
        assert!(OWNED_ALIAS.find(guard).ok_or("expected Some")? < dispatch);
    }
    #[cfg(target_os = "macos")]
    {
        assert!(OWNED_ALIAS.contains(r#"GIT_COMMON_DIR="$common" git --no-lazy-fetch --git-dir="$metadata" --work-tree=. "$@""#));
        assert!(!OWNED_ALIAS.contains("--git-dir=/dev/fd/"));
        assert!(!OWNED_ALIAS.contains("GIT_COMMON_DIR=/dev/fd/"));
    }
    #[cfg(not(target_os = "macos"))]
    assert!(OWNED_ALIAS.contains("--git-dir=/dev/fd/7 --work-tree=."));
    assert_eq!(OWNED_ALIAS.matches("exec 6<.git").count(), 1);
    assert!(!OWNED_ALIAS.contains("cat .git"));
    assert_eq!(
        command[5],
        binding.metadata.to_str().ok_or("expected metadata path")?
    );
    assert_eq!(
        command[7],
        binding.common.to_str().ok_or("expected common path")?
    );
    assert!(!OWNED_ALIAS.contains("eval"));
    Ok(())
}
