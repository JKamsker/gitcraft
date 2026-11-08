//! Deployment provenance (#175 item 9), apart from its drawing: for one commit, the CI runs of
//! it and the artifacts they uploaded, and for each environment the forge lists, the latest
//! successful deployment, whether that is known to include the commit, how fresh that is, and
//! what is queued or under way.
//!
//! A commit is "known included" only when the forge recorded a successful deployment of a
//! commit that is it or descends from it, as the repository's own history says
//! (`merge-base --is-ancestor`). A deployed commit the repository doesn't have is unknown:
//! nothing is fetched to find out. A successful run or a tag is never taken for a deployment.

use std::path::Path;

use super::{Evidence, Provenance};
use crate::ci::deployments::{self, Artifact, DeployState, Deployment, Deployments};
use crate::ci::{Project, Run};
use crate::forge::Forge;

/// So many of the commit's runs have their artifacts read.
pub const MAX_RUNS: usize = 5;

/// Whether an environment is known to run the commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inclusion {
    /// Its latest successful deployment is of the commit (`exact`) or of one descending from it.
    Included { exact: bool },
    /// Its latest successful deployment is of a commit here that doesn't contain it.
    NotIncluded,
    /// Why it can't be told: the deployed commit isn't in the repository, or the successful
    /// deployment wasn't among those read.
    Unknown(String),
    /// Deployments are recorded, and none of those read succeeded.
    NoSuccess,
    /// No deployment to it is recorded (among those read).
    NoRecord,
}

impl Inclusion {
    pub fn label(&self) -> String {
        match self {
            Inclusion::Included { exact: true } => "known included (deployed itself)".to_owned(),
            Inclusion::Included { exact: false } => {
                "known included (a deployed commit contains it)".to_owned()
            }
            Inclusion::NotIncluded => "not in the deployed commit".to_owned(),
            Inclusion::Unknown(why) => format!("unknown: {why}"),
            Inclusion::NoSuccess => "no successful deployment recorded".to_owned(),
            Inclusion::NoRecord => "no deployment recorded".to_owned(),
        }
    }

    /// How it is known: the forge's record and Git's history, or not at all.
    pub fn evidence(&self) -> Evidence {
        match self {
            Inclusion::Included { .. } | Inclusion::NotIncluded => Evidence::Observed,
            _ => Evidence::Unknown,
        }
    }
}

/// How long ago the evidence was reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Freshness {
    /// Within the last day.
    Recent,
    /// Within the last two weeks.
    Days,
    Old,
    /// The forge reported no time.
    Unknown,
}

/// A day, and two weeks, in seconds.
const DAY: i64 = 24 * 3600;
const FORTNIGHT: i64 = 14 * DAY;

pub fn freshness(reported: Option<i64>, now: i64) -> Freshness {
    match reported.map(|r| now - r) {
        None => Freshness::Unknown,
        Some(age) if age < DAY => Freshness::Recent,
        Some(age) if age < FORTNIGHT => Freshness::Days,
        Some(_) => Freshness::Old,
    }
}

/// "reported 3 hours ago", "no report time recorded".
pub fn reported_label(reported: Option<i64>, now: i64) -> String {
    match reported {
        Some(at) => format!("reported {}", crate::git::relative_time(at, now)),
        None => "no report time recorded".to_owned(),
    }
}

/// One environment and what is known of the commit there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvRow {
    pub name: String,
    /// Its page on the forge.
    pub url: String,
    /// Its latest successful deployment.
    pub current: Option<Deployment>,
    pub inclusion: Inclusion,
    /// Deployments after that one, queued or under way, each with whether its commit contains
    /// the commit (`None`: unknown).
    pub in_flight: Vec<(Deployment, Option<bool>)>,
    /// The newest failed deployment after the current one.
    pub failed: Option<Deployment>,
    /// Deployments to it that were read.
    pub records: usize,
}

/// Whether two commit ids name the same commit (either may be abbreviated).
fn same(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.starts_with(b) || b.starts_with(a))
}

/// The environments: those the forge lists, in its order, then any others deployed to.
/// `contains(sha)`: whether deployed commit `sha` is or descends from the commit (`None`: it
/// isn't in the repository).
pub fn environments(
    read: &Deployments,
    commit: &str,
    contains: impl Fn(&str) -> Option<bool>,
) -> Vec<EnvRow> {
    let mut names: Vec<(String, String)> = read
        .environments
        .iter()
        .map(|e| (e.name.clone(), e.url.clone()))
        .collect();
    for d in &read.deployments {
        if !names.iter().any(|(n, _)| *n == d.environment) {
            names.push((d.environment.clone(), d.url.clone()));
        }
    }
    // With the list cut, an environment's older deployments weren't read.
    let cut = read.deployments.len() >= crate::ci::deployments::MAX_DEPLOYMENTS;
    let check = |sha: &str| {
        if same(sha, commit) {
            Some(true)
        } else {
            contains(sha)
        }
    };
    names
        .into_iter()
        .map(|(name, url)| {
            let mine: Vec<&Deployment> = read
                .deployments
                .iter()
                .filter(|d| d.environment == name)
                .collect();
            let at = mine.iter().position(|d| d.state() == DeployState::Success);
            let newer = &mine[..at.unwrap_or(mine.len())];
            let current = at.map(|i| mine[i].clone());
            let inclusion = match &current {
                Some(d) => match check(&d.sha) {
                    Some(true) => Inclusion::Included {
                        exact: same(&d.sha, commit),
                    },
                    Some(false) => Inclusion::NotIncluded,
                    None => Inclusion::Unknown(format!(
                        "the deployed commit {} isn't in this repository (nothing is fetched)",
                        d.sha.get(..7).unwrap_or(&d.sha)
                    )),
                },
                None if cut => Inclusion::Unknown(format!(
                    "no successful deployment among the latest {} read",
                    crate::ci::deployments::MAX_DEPLOYMENTS
                )),
                None if mine.is_empty() => Inclusion::NoRecord,
                None if mine.iter().any(|d| d.state() == DeployState::NotRead) => {
                    Inclusion::Unknown(
                        "its latest successful deployment wasn't among those read".to_owned(),
                    )
                }
                None => Inclusion::NoSuccess,
            };
            EnvRow {
                name,
                url,
                in_flight: newer
                    .iter()
                    .filter(|d| d.state().in_flight())
                    .map(|d| ((*d).clone(), check(&d.sha)))
                    .collect(),
                failed: newer
                    .iter()
                    .find(|d| d.state() == DeployState::Failed)
                    .map(|d| (*d).clone()),
                current,
                inclusion,
                records: mine.len(),
            }
        })
        .collect()
}

/// Whether `deployed` is `commit` or descends from it, by the repository's history alone:
/// `None` when either isn't in the repository (nothing is fetched, not even lazily).
pub fn contains(repo: &Path, commit: &str, deployed: &str) -> Option<bool> {
    let hex = |s: &str| (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex(commit) || !hex(deployed) {
        return None;
    }
    let env = [("GIT_NO_LAZY_FETCH", "1")];
    let here = |sha: &str| {
        crate::git::run_env(
            Some(repo),
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{sha}^{{commit}}"),
            ],
            &env,
        )
        .is_ok()
    };
    if !here(commit) || !here(deployed) {
        return None;
    }
    Some(
        crate::git::run_env(
            Some(repo),
            &["merge-base", "--is-ancestor", commit, deployed],
            &env,
        )
        .is_ok(),
    )
}

/// Whether deployments of `forge` can be read, and if not, what is needed.
pub fn availability(forge: Option<Forge>) -> Result<(), String> {
    match forge {
        None => Err(
            "This repository has no remote on GitHub or GitLab: deployments are read \
                     through gh or glab from the repository's forge."
                .to_owned(),
        ),
        Some(Forge::GitHub | Forge::GitLab) => Ok(()),
        Some(forge) => Err(format!("{}.", deployments::no_deployments(forge))),
    }
}

/// Everything the view shows for one commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flow {
    pub commit: String,
    pub project: Project,
    /// The commit's runs, newest first.
    pub runs: Vec<Run>,
    /// The artifacts of the first [`MAX_RUNS`] runs, by run.
    pub artifacts: Vec<Result<Vec<Artifact>, String>>,
    pub environments: Vec<EnvRow>,
    /// What is worth knowing of how it was read.
    pub notes: Vec<String>,
    pub now: i64,
    pub provenance: Provenance,
}

impl Flow {
    /// Runs that deployments record as having done them: (run index, environment index,
    /// whether it is the environment's current deployment).
    pub fn run_links(&self) -> Vec<(usize, usize, bool)> {
        let mut links = Vec::new();
        for (e, env) in self.environments.iter().enumerate() {
            let records = env
                .current
                .iter()
                .map(|d| (d, true))
                .chain(env.in_flight.iter().map(|(d, _)| (d, false)));
            for (d, current) in records {
                if let Some(r) = d
                    .run
                    .and_then(|id| self.runs.iter().position(|run| run.id == id))
                    && !links.contains(&(r, e, current))
                {
                    links.push((r, e, current));
                }
            }
        }
        links
    }
}

/// Reads the deployments of `project` and the runs and artifacts of `commit`. `subject`: the
/// commit by name (`HEAD`) and id.
pub fn read(
    repo: &Path,
    project: &Project,
    commit: &str,
    subject: (&str, &str),
) -> Result<Flow, String> {
    let recorded = deployments::read(repo, project)?;
    let mut notes = Vec::new();
    let runs = match crate::ci::runs(repo, project, commit) {
        Ok(runs) => runs,
        Err(e) => {
            notes.push(format!("The commit's runs couldn't be read: {e}"));
            Vec::new()
        }
    };
    let artifacts = runs
        .iter()
        .take(MAX_RUNS)
        .map(|run| deployments::artifacts(repo, project, run.id))
        .collect();
    let environments = environments(&recorded, commit, |sha| contains(repo, commit, sha));
    let mut partial = recorded.partial.clone();
    if runs.len() > MAX_RUNS {
        partial.push(format!(
            "the artifacts of only the latest {MAX_RUNS} runs were read"
        ));
    }
    let source = match project.forge {
        Forge::GitLab => "GitLab deployments (glab)",
        _ => "GitHub Deployments API (gh)",
    };
    let mut provenance = Provenance::new(
        source,
        format!(
            "latest {} deployments of {}",
            deployments::MAX_DEPLOYMENTS,
            project.path
        ),
    )
    .tip(subject.0, subject.1);
    if !partial.is_empty() {
        provenance = provenance.partial(partial.join("; "));
    }
    Ok(Flow {
        commit: commit.to_owned(),
        project: project.clone(),
        runs,
        artifacts,
        environments,
        notes,
        now: crate::git::now(),
        provenance,
    })
}

#[cfg(test)]
mod tests {
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
}
