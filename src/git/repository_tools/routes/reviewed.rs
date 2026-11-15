//! Immutable input to the root push checklist, never an endpoint-send capability.
use super::{Force, Snapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PushRoute {
    pub branch: String,
    pub commit: String,
    pub remote: String,
    pub destination: String,
    pub urls: Vec<String>,
    pub lease: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedPush {
    pub snapshot: Snapshot,
    pub route: PushRoute,
}

impl PreparedPush {
    /// Only a locally known symbolic remote HEAD establishes a default branch.
    pub(crate) fn targets_known_default(&self) -> bool {
        let Some(branch) = self.route.destination.strip_prefix("refs/heads/") else {
            return false;
        };
        let head = format!("refs/remotes/{}/HEAD", self.route.remote);
        let target = format!("refs/remotes/{}/{branch}", self.route.remote);
        self.snapshot.refs.lines().any(|line| {
            let mut fields = line.split('\0');
            fields.next() == Some(head.as_str()) && fields.nth(1) == Some(target.as_str())
        })
    }
}

impl From<&Force> for PreparedPush {
    fn from(plan: &Force) -> Self {
        Self {
            snapshot: plan.snapshot.clone(),
            route: PushRoute {
                branch: plan.branch.clone(),
                commit: plan.commit.clone(),
                remote: plan.remote.clone(),
                destination: plan.destination.clone(),
                urls: vec![plan.url.clone()],
                lease: Some(plan.remote_commit.clone()),
            },
        }
    }
}

impl PushRoute {
    pub(crate) fn publication_operations(&self) -> Vec<Vec<String>> {
        let mut ops = self.operations();
        if let Some(push) = ops.last_mut() {
            push.insert(1, "--set-upstream".into());
            if let Some(spec) = push.last_mut() {
                *spec = format!("refs/heads/{}:{}", self.branch, self.destination);
            }
        }
        ops
    }

    pub(crate) fn operations(&self) -> Vec<Vec<String>> {
        let mut push = vec!["push".into(), "--porcelain".into()];
        if let Some(lease) = &self.lease {
            push.push(format!("--force-with-lease={}:{}", self.destination, lease));
        }
        push.extend([
            "--".into(),
            self.remote.clone(),
            format!("{}:{}", self.commit, self.destination),
        ]);
        vec![
            crate::repo::operations::expect_ref_at(
                &format!("refs/heads/{}", self.branch),
                &self.commit,
            ),
            push,
        ]
    }
}

#[cfg(test)]
mod continuation_recording {
    use super::*;

    #[test]
    fn known_default_requires_the_selected_remotes_exact_symbolic_head() {
        let mut prepared = PreparedPush {
            snapshot: Snapshot::recording(),
            route: PushRoute {
                branch: "topic".into(),
                commit: "a".repeat(40),
                remote: "alice/team".into(),
                destination: "refs/heads/release/töpíc".into(),
                urls: vec![],
                lease: None,
            },
        };
        for (name, target, expected) in [
            ("alice/team", "alice/team/release/töpíc", true),
            ("origin", "origin/release/töpíc", false),
            ("alice/team", "alice/team/topic", false),
            ("alice/team", "", false),
        ] {
            prepared.snapshot.refs = format!(
                "refs/remotes/{name}/HEAD\0{}\0{}\n",
                prepared.route.commit,
                if target.is_empty() {
                    String::new()
                } else {
                    format!("refs/remotes/{target}")
                }
            );
            assert_eq!(prepared.targets_known_default(), expected);
        }
        prepared.snapshot.refs.clear();
        assert!(!prepared.targets_known_default());
    }

    #[test]
    fn plain_and_force_actions_keep_the_selected_push_host_oid_and_lease() {
        for lease in [None, Some("b".repeat(40))] {
            let route = PushRoute {
                branch: "topic".into(),
                commit: "a".repeat(40),
                remote: "publish-host".into(),
                destination: "refs/heads/release".into(),
                urls: vec!["https://publish.invalid/team/repo".into()],
                lease,
            };
            let ops = route.operations();
            assert!(crate::push_preflight::has_push(&ops));
            assert_eq!(
                ops[0],
                crate::repo::operations::expect_ref_at("refs/heads/topic", &route.commit)
            );
            let push = &ops[1];
            assert_eq!(
                &push[push.len() - 2..],
                [
                    "publish-host",
                    &format!("{}:refs/heads/release", route.commit)
                ]
            );
            assert_eq!(
                push.iter().any(|a| a.starts_with("--force-with-lease=")),
                route.lease.is_some()
            );
            if let Some(lease) = &route.lease {
                assert!(push.contains(&format!("--force-with-lease=refs/heads/release:{lease}")));
            }
            assert!(!push.iter().any(|arg| arg == "pull-host"));
            // This action is still an ordinary push: only root preflight can
            // replace it with opaque APPROVED execution evidence.
            assert!(!ops.iter().any(|op| {
                op.first()
                    .is_some_and(|m| m == crate::push_preflight::APPROVED)
            }));
        }
    }
}
