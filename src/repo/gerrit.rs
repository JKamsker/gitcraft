//! Gerrit dialogs and graph links, using the same operation runner as an ordinary push.

use super::{Action, Dialog, RepoTab};

impl RepoTab {
    pub(super) fn review_dialog(&self) -> Option<Dialog> {
        let server = self.gerrit.as_ref()?;
        if self.refs.head.is_empty() {
            return None;
        }
        let targets: Vec<(String, String)> = self
            .refs
            .remotes
            .iter()
            .filter(|(name, _)| name == &server.remote)
            .flat_map(|(_, branches)| branches)
            .filter_map(|b| {
                let name = super::sidebar::remote_short(&b.name, &server.remote)?;
                crate::gerrit::valid_branch(name).then(|| (name.to_owned(), b.target.clone()))
            })
            .collect();
        let upstream = self
            .refs
            .current
            .as_ref()
            .and_then(|name| self.refs.local.iter().find(|b| &b.name == name))
            .and_then(|b| self.split_upstream(&b.upstream))
            .filter(|(remote, _)| remote == &server.remote)
            .map(|(_, name)| name.to_owned());
        let target = upstream
            .filter(|n| targets.iter().any(|(name, _)| name == n))
            .or_else(|| {
                ["main", "master"]
                    .into_iter()
                    .find(|n| targets.iter().any(|(name, _)| name == n))
                    .map(str::to_owned)
            })
            .or_else(|| targets.first().map(|(name, _)| name.clone()))
            .unwrap_or_default();
        Some(Dialog::GerritReview {
            remote: server.remote.clone(),
            head: self.refs.head.clone(),
            targets,
            target,
            topic: String::new(),
        })
    }

    pub(super) fn gerrit_menu(
        &self,
        ui: &mut egui::Ui,
        commit: &crate::git::Commit,
        acts: &mut Vec<Action>,
    ) {
        if let (Some(server), Some(id)) = (&self.gerrit, &commit.change_id)
            && ui.button("Open Gerrit change").on_hover_text(id).clicked()
        {
            acts.push(Action::OpenUrl(server.change_url(id)));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::gerrit::*;
    use crate::git;
    use std::path::{Path, PathBuf};
    const ID: &str = "I0123456789abcdef0123456789abcdef01234567";
    fn repo(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("gitcraft-gerrit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        assert!(std::fs::create_dir_all(&path).is_ok());
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.org"],
            vec!["commit", "--allow-empty", "-qm", "Base without Change-Id"],
        ] {
            assert!(git::run(Some(&path), &args).is_ok());
        }
        path
    }
    fn commit(repo: &Path, message: &str) {
        assert!(std::fs::write(repo.join("review.txt"), message).is_ok());
        assert!(git::run(Some(repo), &["add", "review.txt"]).is_ok());
        assert!(git::run(Some(repo), &["commit", "--allow-empty", "-qm", message]).is_ok());
    }

    /// A local receiver stands in for Gerrit's receive-pack: no real review or credentials.
    #[test]
    fn submission_pushes_to_the_review_namespace_and_amendment_uploads_the_next_object() {
        use crate::repo::OpKind;
        use crate::repo::operations::run_steps;
        let path = repo("upload");
        let receiver = path.join("receiver.git");
        let remote = receiver.to_string_lossy().into_owned();
        let base = git::head(&path);
        assert!(git::run(None, &["init", "--bare", "-q", &remote]).is_ok());
        assert!(git::run(Some(&path), &["remote", "add", "origin", &remote]).is_ok());
        assert!(git::run(Some(&path), &["push", "origin", "main"]).is_ok());
        commit(&path, &format!("Review\n\nChange-Id: {ID}"));
        let first = git::head(&path);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert!(
                git::run(
                    Some(&receiver),
                    &["config", "receive.advertisePushOptions", "true"]
                )
                .is_ok()
            );
            let hook = receiver.join("hooks/pre-receive");
            assert!(
                std::fs::write(
                    &hook,
                    "#!/bin/sh\nprintf '%s' \"$GIT_PUSH_OPTION_0\" > topic.txt\ncat > /dev/null\n"
                )
                .is_ok()
            );
            assert!(
                std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).is_ok()
            );
        }
        let topic = if cfg!(unix) {
            "topic,submit%ready=yes"
        } else {
            ""
        };
        let plan = review_ops("origin", "main", topic, &first, &base).unwrap_or_default();
        assert!(run_steps(&path, &plan, OpKind::plain(), None).0.is_ok());
        assert_eq!(git::resolve(&receiver, "refs/for/main"), Ok(first.clone()));
        #[cfg(unix)]
        assert_eq!(
            std::fs::read_to_string(receiver.join("topic.txt"))
                .ok()
                .as_deref(),
            Some("topic=topic,submit%ready=yes")
        );
        assert_eq!(git::resolve(&receiver, "refs/heads/main"), Ok(base.clone()));
        // A new patch set has the same Change-Id, but a different commit object. Delete the
        // fixture's ref: Gerrit consumes refs/for and doesn't keep this as an ordinary ref.
        assert!(git::run(Some(&receiver), &["update-ref", "-d", "refs/for/main"]).is_ok());
        let amend = vec![vec![AMEND.to_owned(), "Revised review".to_owned()]];
        assert!(run_steps(&path, &amend, OpKind::plain(), None).0.is_ok());
        let second = git::head(&path);
        assert_ne!(first, second);
        let plan = review_ops("origin", "main", "", &second, &base).unwrap_or_default();
        assert!(run_steps(&path, &plan, OpKind::plain(), None).0.is_ok());
        assert_eq!(git::resolve(&receiver, "refs/for/main"), Ok(second.clone()));
        assert_eq!(
            git::log(&path, None, 0, 1)
                .ok()
                .and_then(|list| list.first().and_then(|c| c.change_id.clone())),
            Some(ID.to_owned())
        );
        assert!(git::run(Some(&receiver), &["update-ref", "-d", "refs/for/main"]).is_ok());
        commit(&path, "Missing Change-Id");
        let bad_plan =
            review_ops("origin", "main", "", &git::head(&path), &base).unwrap_or_default();
        assert!(
            run_steps(&path, &bad_plan, OpKind::plain(), None)
                .0
                .is_err()
        );
        assert!(git::resolve(&receiver, "refs/for/main").is_err());
        let _ = std::fs::remove_dir_all(path);
    }
}

#[cfg(test)]
mod flow_tests {
    use super::*;
    use crate::git::{Branch, Remote};

    #[test]
    fn review_targets_come_only_from_the_detected_remote_and_prefer_the_upstream() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/gerrit-targets".into(), &ctx);
        tab.gerrit = Some(crate::gerrit::Server {
            remote: "review".into(),
            web: "https://review.example.org".into(),
            project: "app".into(),
        });
        tab.remotes = vec![Remote {
            name: "review".into(),
            url: String::new(),
        }];
        tab.refs.head = "a".repeat(40);
        tab.refs.current = Some("feature".into());
        let branch = |name: &str, upstream: &str| Branch {
            name: name.to_owned(),
            target: "b".repeat(40),
            upstream: upstream.to_owned(),
            is_head: false,
            track: String::new(),
        };
        tab.refs.local = vec![branch("feature", "review/stable")];
        tab.refs.remotes = vec![
            ("other".into(), vec![branch("other/wrong", "")]),
            (
                "review".into(),
                vec![branch("review/main", ""), branch("review/stable", "")],
            ),
        ];
        let dialog = tab.review_dialog();
        assert!(dialog.is_some());
        let Some(Dialog::GerritReview {
            remote,
            target,
            targets,
            head,
            ..
        }) = dialog
        else {
            return;
        };
        assert_eq!(remote, "review");
        assert_eq!(target, "stable");
        assert_eq!(head, "a".repeat(40));
        assert_eq!(
            targets
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            vec!["main", "stable"]
        );
    }
}
