//! A re-run or cancel answers the view it was pressed in (#195): an older view's answer does
//! not release the newer view's buttons while its own request still runs.

use super::*;

/// A project whose builds gitcraft does not read, so acting on its runs fails at once
/// without going on the network.
fn offline() -> Project {
    Project {
        forge: crate::forge::Forge::Forgejo,
        host: "codeberg.org".to_owned(),
        path: "owner/repo".to_owned(),
    }
}

/// Shows commit `sha`'s builds with run `id`, named `name`, selected.
fn show(tab: &mut RepoTab, sha: &str, id: u64, name: &str, ctx: &egui::Context) {
    tab.open_ci(sha.to_owned(), None, ctx);
    if let Some(view) = &mut tab.ci_view {
        let run = Run {
            id,
            name: name.to_owned(),
            number: id,
            ..Run::default()
        };
        view.loading = false;
        view.project = Some(offline());
        view.run = Some(id);
        view.runs = Some(Ok(Builds {
            runs: vec![(offline(), run)],
            errors: vec![],
        }));
    }
}

fn acting(tab: &RepoTab) -> bool {
    tab.ci_view.as_ref().is_some_and(|v| v.acting.is_some())
}

/// Presses `act` in the view shown and returns its answer, without handing it to the tab.
fn press(tab: &mut RepoTab, act: CiAct, ctx: &egui::Context) -> Result<Msg, String> {
    let ((), jobs) = super::super::messages::record_jobs(|| tab.ci_act(act, ctx));
    let [job]: [_; 1] = jobs
        .try_into()
        .map_err(|jobs: Vec<_>| format!("{} jobs submitted", jobs.len()))?;
    Ok(job())
}

#[test]
fn an_older_views_answer_leaves_the_newer_views_action_running() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/ci-act-tests".into(), &ctx);
    show(&mut tab, "aaaaaaa", 1, "first-run", &ctx);
    let older = press(&mut tab, CiAct::Cancel, &ctx)?;
    assert!(acting(&tab));
    show(&mut tab, "bbbbbbb", 2, "second-run", &ctx);
    assert!(!acting(&tab), "a new view has nothing under way");
    let newer = press(&mut tab, CiAct::RerunFailed, &ctx)?;
    assert!(acting(&tab));
    tab.handle(older, &ctx);
    assert!(
        acting(&tab),
        "the first view's answer released the second's buttons"
    );
    let said = tab.status.as_ref().map(|n| n.raw().0.clone());
    assert!(
        said.as_deref().is_some_and(|s| s.contains("first-run")),
        "the first action's outcome is still said: {said:?}"
    );
    tab.handle(newer, &ctx);
    assert!(!acting(&tab), "its own answer releases the buttons");
    assert!(tab.ci_view.as_ref().is_some_and(|v| v.next.is_some()));
    Ok(())
}
