//! Gerrit dialogs and graph links, using the same operation runner as an ordinary push.

use super::{Action, Dialog, RepoTab};

impl RepoTab {
    pub(super) fn review_dialog(&self) -> Option<Dialog> {
        self.review_dialog_for(self.refs.current.as_deref())
    }

    pub(super) fn review_dialog_for(&self, branch: Option<&str>) -> Option<Dialog> {
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
        let upstream = branch
            .and_then(|name| self.refs.local.iter().find(|b| b.name == name))
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
            skip_hooks: false,
            web_warning: server.web_warning.clone(),
        })
    }

    pub(super) fn gerrit_menu(
        &self,
        ui: &mut egui::Ui,
        commit: &crate::git::Commit,
        acts: &mut Vec<Action>,
    ) {
        if let (Some(server), Some(id)) = (&self.gerrit, &commit.change_id)
            && crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2582"))
                .icon("eye")
                .show(ui)
                .on_hover_text(id)
                .clicked()
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
        let amend = vec![vec![
            AMEND.to_owned(),
            "commit".to_owned(),
            "--amend".to_owned(),
            "-m".to_owned(),
            "Revised review".to_owned(),
        ]];
        assert!(run_steps(&path, &amend, OpKind::plain(), None).0.is_ok());
        let second = git::head(&path);
        assert_ne!(first, second);
        let plan = review_ops("origin", "main", "", &second, &base).unwrap_or_default();
        assert!(run_steps(&path, &plan, OpKind::plain(), None).0.is_ok());
        assert_eq!(git::resolve(&receiver, "refs/for/main"), Ok(second.clone()));
        assert_eq!(
            git::log(&path, None, Default::default(), 0, 1)
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
    fn review_targets_come_only_from_the_detected_remote_and_prefer_the_upstream()
    -> Result<(), String> {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/gerrit-targets".into(), &ctx);
        tab.gerrit = Some(crate::gerrit::Server {
            remote: "review".into(),
            web: "https://review.example.org".into(),
            project: "app".into(),
            web_warning: None,
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
        tab.refs.local.push(branch("another", "review/main"));
        let (
            _,
            Action::Dialog(Dialog::GerritReview {
                target: pushed_target,
                ..
            }),
        ) = tab.push_branch(&tab.refs.local[1]).ok_or("review action")?
        else {
            return Err("expected review dialog".to_owned());
        };
        assert_eq!(pushed_target, "main");
        let dialog = tab.review_dialog();
        assert!(matches!(&dialog, Some(Dialog::GerritReview { .. })));
        let Some(Dialog::GerritReview {
            remote,
            target,
            targets,
            head,
            ..
        }) = dialog
        else {
            return Err("expected review dialog".to_owned());
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
        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::git;
    use std::time::{Duration, Instant};

    fn settle(tab: &mut RepoTab, ctx: &egui::Context) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            tab.poll(ctx);
            if tab.busy_op.is_none() && !tab.loading {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            tab.busy_op.is_none() && !tab.loading,
            "background operation did not finish"
        );
    }

    #[cfg(unix)]
    #[test]
    fn amend_preserves_signoff_and_skips_only_the_requested_commit_hooks() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let path =
            std::env::temp_dir().join(format!("gitcraft-gerrit-options-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        let run = |args: &[&str]| git::run(Some(&path), args);
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Review Author"],
            vec!["config", "user.email", "review@example.invalid"],
            vec!["config", "commit.gpgSign", "false"],
            vec![
                "remote",
                "add",
                "origin",
                "https://gerrit.example.invalid/project",
            ],
            vec![
                "commit",
                "--allow-empty",
                "-qm",
                "Original\n\nChange-Id: I0123456789abcdef0123456789abcdef01234567",
            ],
        ] {
            run(&args)?;
        }
        std::fs::write(path.join("review.txt"), "review\n").map_err(|e| e.to_string())?;
        run(&["add", "review.txt"])?;
        run(&["commit", "--amend", "--no-edit"])?;
        let hook = path.join(".git/hooks/pre-commit");
        std::fs::write(&hook, "#!/bin/sh\nexit 1\n").map_err(|e| e.to_string())?;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(path.clone(), &ctx);
        tab.reload(&ctx);
        settle(&mut tab, &ctx);
        assert!(
            tab.gerrit.is_some(),
            "status={:?}, error={:?}, loading={}, head={}, remotes={:?}",
            tab.status,
            tab.load_error,
            tab.loading,
            tab.refs.head,
            tab.remotes
        );
        tab.dco_act(crate::repo::dco::DcoAct::SetSignOff(true), &ctx);
        tab.amend = true;
        tab.commit_summary = "Amended".to_owned();
        tab.commit_body = "Keep this paragraph.".to_owned();
        tab.skip_hooks.commit = true;
        let before = git::head(&path);
        tab.commit(&ctx);
        settle(&mut tab, &ctx);
        let amended = git::head(&path);
        assert_ne!(amended, before, "{:?}", tab.status);
        assert!(!tab.skip_hooks.commit);
        let message = git::head_message(&path)?;
        assert!(message.contains("Keep this paragraph."));
        assert!(
            message.contains("Signed-off-by: Review Author <review@example.invalid>"),
            "{message}"
        );
        assert_eq!(message.matches("Change-Id:").count(), 1);
        assert_eq!(
            crate::gerrit::change_id(&message)?,
            "I0123456789abcdef0123456789abcdef01234567"
        );
        tab.amend = true;
        tab.commit_summary = "This one must run its hook".to_owned();
        tab.commit(&ctx);
        settle(&mut tab, &ctx);
        assert_eq!(git::head(&path), amended);
        assert!(tab.status.as_deref().is_some_and(|(_, failed)| *failed));
        std::fs::remove_file(&hook).map_err(|e| e.to_string())?;
        tab.reword_commit(
            amended.clone(),
            "Reworded\n\nCo-authored-by: Co Author <co@example.invalid>".to_owned(),
            &ctx,
        );
        settle(&mut tab, &ctx);
        let reworded = git::head(&path);
        assert_ne!(reworded, amended);
        let message = git::head_message(&path)?;
        assert_eq!(
            crate::gerrit::change_id(&message)?,
            "I0123456789abcdef0123456789abcdef01234567"
        );
        let trailers = run(&[
            "show",
            "-s",
            "--format=%(trailers:key=Co-authored-by)",
            "HEAD",
        ])?;
        assert!(trailers.contains("Co Author"), "{message}");
        tab.reword_commit(
            reworded.clone(),
            "Changed identity\n\nChange-Id: I1123456789abcdef0123456789abcdef01234567".to_owned(),
            &ctx,
        );
        settle(&mut tab, &ctx);
        assert_eq!(git::head(&path), reworded);
        assert!(tab.status.as_deref().is_some_and(|(_, failed)| *failed));
        run(&["checkout", "-qb", "external"])?;
        tab.amend = true;
        tab.commit_summary = "Must not move external".to_owned();
        tab.commit(&ctx);
        settle(&mut tab, &ctx);
        assert_eq!(git::head(&path), reworded);
        assert_eq!(git::resolve(&path, "main")?, reworded);
        assert!(tab.status.as_deref().is_some_and(|(_, failed)| *failed));
        drop(tab);
        let _ = std::fs::remove_dir_all(path);
        Ok(())
    }

    #[test]
    fn sync_and_skipped_push_require_review_confirmation() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/gerrit-review-actions".into(), &ctx);
        tab.gerrit = Some(crate::gerrit::Server {
            remote: "origin".into(),
            web: "https://gerrit.example.invalid".into(),
            project: "project".into(),
            web_warning: None,
        });
        tab.refs.head = "a".repeat(40);
        tab.sync(&ctx);
        assert!(matches!(
            tab.dialog,
            Some(Dialog::GerritReview {
                skip_hooks: false,
                ..
            })
        ));
        assert!(tab.busy_op.is_none());
        assert!(tab.last_op.is_none());
        tab.push_skipping_hooks(&ctx);
        assert!(matches!(
            tab.dialog,
            Some(Dialog::GerritReview {
                skip_hooks: true,
                ..
            })
        ));
        assert!(tab.busy_op.is_none());
        assert!(tab.last_op.is_none());
        assert!(
            !tab.additional_palette_items(&ctx)
                .iter()
                .any(|(_, _, a)| matches!(a, Action::Sync))
        );
        assert!(!tab.palette_items(&ctx).iter().any(|i| matches!(
            i.run,
            crate::repo::palette::PaletteRun::Action(Action::Sync)
        )));
    }
}
