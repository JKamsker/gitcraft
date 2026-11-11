use super::*;

fn project(path: &str) -> Project {
    Project {
        forge: crate::forge::Forge::GitLab,
        host: "gitlab.com".to_owned(),
        path: path.to_owned(),
    }
}

/// Empty upstream results do not conceal an older fork build; run IDs may overlap.
#[test]
fn builds_search_all_candidates_and_retain_each_runs_project() -> Result<(), String> {
    let projects = vec![
        project("up/repo"),
        project("me/repo"),
        project("other/repo"),
    ];
    let builds = read_builds(projects, |p| {
        Ok(if p.path == "up/repo" {
            vec![]
        } else {
            vec![Run {
                id: 7,
                ..Run::default()
            }]
        })
    })?;
    let owners: Vec<_> = builds
        .runs
        .iter()
        .map(|(p, run)| (p.path.as_str(), run.id))
        .collect();
    assert_eq!(owners, [("me/repo", 7), ("other/repo", 7)]);
    assert!(builds.errors.is_empty());
    Ok(())
}

/// A partial read shows its error alongside readable builds rather than claiming no CI.
#[test]
fn builds_report_partial_project_failures() -> Result<(), String> {
    let builds = read_builds(vec![project("up/repo"), project("me/repo")], |p| {
        if p.path == "up/repo" {
            Err("offline".to_owned())
        } else {
            Ok(vec![Run {
                id: 7,
                ..Run::default()
            }])
        }
    })?;
    assert_eq!(builds.runs.len(), 1);
    assert_eq!(builds.errors, ["up/repo: offline"]);
    assert!(read_builds(vec![project("up/repo")], |_| Err("offline".to_owned())).is_err());
    Ok(())
}

/// Refreshing commit status cannot redirect a loaded pipeline's job/action requests.
#[test]
fn loaded_runs_keep_their_project_when_the_commit_source_changes() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/ci-project-test".into(), &ctx);
    tab.open_ci("abc".to_owned(), None, &ctx);
    if let Some(view) = &mut tab.ci_view {
        view.project = Some(project("me/repo"));
        view.run = Some(7);
        view.runs = Some(Ok(Builds {
            runs: vec![
                (
                    project("up/repo"),
                    Run {
                        id: 7,
                        name: "up".to_owned(),
                        ..Run::default()
                    },
                ),
                (
                    project("me/repo"),
                    Run {
                        id: 7,
                        name: "fork".to_owned(),
                        ..Run::default()
                    },
                ),
            ],
            errors: vec![],
        }));
    }
    assert_eq!(tab.ci_view_project(), Some(project("me/repo")));
    assert_eq!(
        tab.ci_view
            .as_ref()
            .and_then(|v| v.shown_run())
            .map(|r| r.name.as_str()),
        Some("fork")
    );
    // Refreshing status from another project does not change the loaded run's identity.
    tab.ci_arrived(
        false,
        vec![(
            project("up/repo"),
            Ok(std::collections::HashMap::from([(
                "abc".to_owned(),
                ci::Status {
                    state: CheckState::Passed,
                    checks: vec![],
                },
            )])),
        )],
    );
    assert_eq!(tab.ci_view_project(), Some(project("me/repo")));
}

/// A poll of a running build that fails (the network blinked) is tried again: the view
/// does not stop following the build.
#[test]
fn a_failed_poll_of_a_running_build_is_tried_again() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/ci-poll-test".into(), &ctx);
    tab.ci_view = Some(CiView {
        sha: "abc".to_owned(),
        request: 7,
        loading: true,
        runs: None,
        project: None,
        run: Some(1),
        jobs: None,
        job: None,
        log: None,
        log_request: 0,
        next: None,
        was_running: true,
        acting: false,
    });
    tab.ci_runs_arrived(7, Err("timed out".to_owned()), &ctx);
    assert!(tab.ci_view.as_ref().is_some_and(|v| v.next.is_some()));
    if let Some(view) = &mut tab.ci_view {
        view.next = None;
    }
    tab.ci_jobs_arrived(7, 1, Err("timed out".to_owned()), &ctx);
    assert!(tab.ci_view.as_ref().is_some_and(|v| v.next.is_some()));
}
