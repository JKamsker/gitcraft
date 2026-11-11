use super::*;
#[test]
fn fork_objects_use_source_branch_without_destination_credential_rebinding() -> Result<(), String> {
    let mut d = PrDetails {
        forge: Some(Forge::Bitbucket),
        url: "https://bitbucket.org/team/repo/pull-requests/7".into(),
        base_branch: "main".into(),
        head_branch: "topic".into(),
        base_oid: "a".repeat(40),
        head_oid: "b".repeat(40),
        extras: PrExtras {
            head_repo: Some("contributor/fork".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let plan = bitbucket_fetches(&d, "origin")?;
    assert_eq!(plan[0].0[3], "origin");
    assert!(plan[0].1);
    assert_eq!(plan[1].0[3], "https://bitbucket.org/contributor/fork.git");
    assert_eq!(plan[1].0[4], "refs/heads/topic");
    assert!(!plan[1].1);
    assert!(plan.iter().all(|(args, _)| {
        !args
            .iter()
            .any(|a| a.contains("refs/pull-requests/") || a.contains(":refs/"))
    }));
    d.extras.head_repo = Some("team/repo".into());
    let own = bitbucket_fetches(&d, "selected-push-remote")?;
    assert_eq!(own[1].0[3], "selected-push-remote");
    assert!(own[1].1);
    d.extras.head_repo = Some("../../foreign".into());
    assert!(bitbucket_fetches(&d, "origin").is_err());
    d.extras.head_repo = Some("team/repo".into());
    d.url = "https://bitbucket.org.evil/team/repo/pull-requests/7".into();
    assert!(bitbucket_fetches(&d, "origin").is_err());
    Ok(())
}
