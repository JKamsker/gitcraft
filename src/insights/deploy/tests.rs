use super::*;
use crate::ci::deployments::Environment;

fn deployment(id: u64, env: &str, sha: &str, state: DeployState) -> Deployment {
    Deployment {
        id,
        environment: env.to_owned(),
        sha: sha.to_owned(),
        state: Some(state),
        reported: Some(1000),
        ..Deployment::default()
    }
}

fn recorded(list: Vec<Deployment>) -> Deployments {
    Deployments {
        environments: ["staging", "production", "preview"]
            .map(|n| Environment {
                name: n.to_owned(),
                url: format!("https://github.com/o/r/deployments/{n}"),
            })
            .to_vec(),
        deployments: list,
        partial: Vec::new(),
    }
}

#[test]
fn inclusion_follows_the_latest_successful_deployment_only() {
    let read = recorded(vec![
        deployment(5, "production", "c2", DeployState::InProgress),
        deployment(4, "production", "c1", DeployState::Failed),
        deployment(3, "production", "c0", DeployState::Success),
        deployment(2, "staging", "x9", DeployState::Success),
        deployment(1, "production", "c1", DeployState::Inactive),
        deployment(6, "qa", "zz", DeployState::NotRead),
    ]);
    // c1 and c2 contain the commit, c0 doesn't, x9 isn't here.
    let rows = environments(&read, "c1", |sha| match sha {
        "c2" => Some(true),
        "c0" => Some(false),
        _ => None,
    });
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["staging", "production", "preview", "qa"]);
    assert!(matches!(rows[0].inclusion, Inclusion::Unknown(_)));
    let prod = &rows[1];
    assert_eq!(prod.inclusion, Inclusion::NotIncluded);
    assert_eq!(prod.current.as_ref().map(|d| d.id), Some(3));
    assert_eq!(prod.in_flight.len(), 1);
    assert_eq!(prod.in_flight[0].1, Some(true), "c2 would include it");
    assert_eq!(prod.failed.as_ref().map(|d| d.id), Some(4));
    assert_eq!(prod.records, 4);
    // Listed, never deployed to: no record, not "not deployed".
    assert_eq!(rows[2].inclusion, Inclusion::NoRecord);
    assert_eq!(rows[2].inclusion.label(), "no deployment recorded");
    assert!(matches!(rows[3].inclusion, Inclusion::Unknown(_)));
}

/// With as many deployments read as are asked for, older ones weren't: an environment
/// without a successful one among them isn't known never to have had one.
#[test]
fn a_cut_list_leaves_environments_without_success_unknown() {
    let max = crate::ci::deployments::MAX_DEPLOYMENTS as u64;
    let read = recorded(
        (0..max)
            .map(|i| deployment(i, "staging", "c9", DeployState::Failed))
            .collect(),
    );
    let rows = environments(&read, "c1", |_| None);
    assert!(matches!(rows[0].inclusion, Inclusion::Unknown(_)));
    assert!(matches!(rows[1].inclusion, Inclusion::Unknown(_)));
}

#[test]
fn a_deployment_of_the_commit_itself_is_included_exactly() {
    let read = recorded(vec![deployment(
        1,
        "staging",
        "abc1234",
        DeployState::Success,
    )]);
    let rows = environments(&read, "abc1234def", |_| None);
    assert_eq!(rows[0].inclusion, Inclusion::Included { exact: true });
    assert_eq!(rows[0].inclusion.evidence(), Evidence::Observed);
    let failed = recorded(vec![deployment(1, "staging", "abc", DeployState::Failed)]);
    let rows = environments(&failed, "abc", |_| Some(true));
    assert_eq!(rows[0].inclusion, Inclusion::NoSuccess);
}

#[test]
fn evidence_ages_from_recent_to_old() {
    assert_eq!(freshness(Some(100), 200), Freshness::Recent);
    assert_eq!(freshness(Some(0), 3 * DAY), Freshness::Days);
    assert_eq!(freshness(Some(0), 30 * DAY), Freshness::Old);
    assert_eq!(freshness(None, 30), Freshness::Unknown);
    assert_eq!(reported_label(None, 0), "no report time recorded");
}

#[test]
fn forges_without_deployments_are_unavailable_with_what_is_needed() {
    assert!(availability(Some(Forge::GitHub)).is_ok());
    assert!(availability(Some(Forge::GitLab)).is_ok());
    assert!(availability(None).is_err_and(|e| e.contains("GitHub or GitLab")));
    assert!(availability(Some(Forge::Forgejo)).is_err_and(|e| e.contains("Forgejo")));
}

#[test]
fn runs_link_to_the_environments_their_deployments_name() {
    let mut current = deployment(1, "staging", "abc", DeployState::Success);
    current.run = Some(77);
    let mut rolling = deployment(2, "production", "abc", DeployState::Queued);
    rolling.run = Some(77);
    let read = recorded(vec![rolling, current]);
    let flow = Flow {
        commit: "abc".to_owned(),
        project: Project {
            forge: Forge::GitHub,
            host: "github.com".to_owned(),
            path: "o/r".to_owned(),
        },
        runs: vec![
            Run {
                id: 5,
                ..Run::default()
            },
            Run {
                id: 77,
                ..Run::default()
            },
        ],
        artifacts: Vec::new(),
        environments: environments(&read, "abc", |_| None),
        notes: Vec::new(),
        now: 0,
        provenance: Provenance::default(),
    };
    assert_eq!(flow.run_links(), [(1, 0, true), (1, 1, false)]);
}

/// A commit becomes known included once a later deployed commit descends from it, by the
/// repository's own history; a deployed commit the repository lacks stays unknown.
#[test]
fn a_commit_is_included_once_a_deployed_descendant_exists() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("deploy-inclusion")?;
    let base = repo.commit("base", &[("a.txt", Some("1"))])?;
    let fix = repo.commit("fix", &[("a.txt", Some("2"))])?;
    repo.git(&["checkout", "-q", "-b", "side", &base])?;
    let side = repo.commit("side", &[("b.txt", Some("x"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    let check = |sha: &str| contains(repo.path(), &fix, sha);
    let before = recorded(vec![deployment(
        1,
        "production",
        &base,
        DeployState::Success,
    )]);
    assert_eq!(
        environments(&before, &fix, check)[1].inclusion,
        Inclusion::NotIncluded
    );
    let other = recorded(vec![deployment(
        2,
        "production",
        &side,
        DeployState::Success,
    )]);
    assert_eq!(
        environments(&other, &fix, check)[1].inclusion,
        Inclusion::NotIncluded
    );
    let later = repo.commit("later", &[("c.txt", Some("y"))])?;
    let after = recorded(vec![
        deployment(3, "production", &later, DeployState::Success),
        deployment(1, "production", &base, DeployState::Inactive),
    ]);
    assert_eq!(
        environments(&after, &fix, check)[1].inclusion,
        Inclusion::Included { exact: false }
    );
    let missing = "f".repeat(40);
    assert_eq!(contains(repo.path(), &fix, &missing), None);
    let gone = recorded(vec![deployment(
        4,
        "production",
        &missing,
        DeployState::Success,
    )]);
    assert!(matches!(
        environments(&gone, &fix, check)[1].inclusion,
        Inclusion::Unknown(_)
    ));
    assert_eq!(contains(repo.path(), &fix, "--all"), None);
    Ok(())
}
