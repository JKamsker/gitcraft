//! Backport rehearsals never move a user's branch: patches and trusted regression commands
//! run in owned detached scratch worktrees. Patch application and behavior stay separate.
use super::{store, tools::ToolsAct};
use crate::{
    git,
    repo::{
        Action, RepoTab,
        drag_drop::{DragItem, Payload},
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Observation {
    pub commit: String,
    pub tree: String,
    pub command: String,
    pub code: Option<i32>,
    pub stopped: bool,
    pub lines: Vec<String>,
    pub machine: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Row {
    target: String,
    tip: String,
    original: String,
    source: String,
    applied: Option<String>,
    error: Option<String>,
    test: Option<Observation>,
    adaptation: Option<(String, String)>,
    cleanup: Option<String>,
}
#[derive(Clone, Default)]
pub(crate) struct Plan {
    fix: String,
    targets: String,
    command: String,
    trusted: bool,
}
#[derive(Default)]
pub(crate) struct State {
    plan: Plan,
    busy: bool,
    stop: git::RunStop,
    rows: Option<Result<Vec<Row>, String>>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.stop.stop();
    }
}
#[derive(Clone)]
pub(crate) enum Act {
    Run(Plan),
    Stop,
    Load,
}
pub(super) fn resolve(repo: &Path, rev: &str) -> Result<String, String> {
    git::run(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{}^{{commit}}", rev.trim()),
        ],
    )
    .map(|s| s.trim().into())
}
pub(super) fn observe(
    repo: &Path,
    commit: &str,
    tree: &str,
    command: &str,
    stop: &git::RunStop,
) -> Result<Observation, String> {
    let (end, lines) = git::run_command(repo, command, &stop.command_stopper())?;
    Ok(Observation {
        commit: commit.into(),
        tree: tree.into(),
        command: command.into(),
        code: end.code,
        stopped: end.stopped,
        lines,
        machine: crate::remote::parse(repo).map_or_else(|| "local".into(), |t| t.host.label()),
    })
}
fn patch(repo: &Path, rev: &str) -> Result<String, String> {
    git::run(
        Some(repo),
        &[
            "show",
            "--format=",
            "--no-ext-diff",
            "--no-textconv",
            rev,
            "--",
        ],
    )
}
fn targets(repo: &Path, text: &str) -> Result<Vec<(String, String, Option<String>)>, String> {
    let mut targets = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
        let (target, adaptation) = line
            .split_once('=')
            .map_or((line, None), |(a, b)| (a.trim(), Some(b.trim())));
        targets.insert(
            target.to_owned(),
            (
                resolve(repo, target)?,
                adaptation.map(|rev| resolve(repo, rev)).transpose()?,
            ),
        );
    }
    if targets.is_empty() || targets.len() > 32 {
        return Err("Choose between 1 and 32 target revisions, one per line".into());
    }
    Ok(targets
        .into_iter()
        .map(|(name, (tip, adaptation))| (name, tip, adaptation))
        .collect())
}
fn evidence_roots(
    fix: &str,
    target: &str,
    source: &str,
    tree: Option<&str>,
) -> Vec<(String, String)> {
    let mut roots = vec![];
    for oid in [fix, target, source] {
        roots.push((
            format!("refs/gitcraft/backport-objects/commits/{oid}"),
            oid.into(),
        ));
    }
    if let Some(tree) = tree {
        roots.push((
            format!("refs/gitcraft/backport-objects/trees/{tree}"),
            tree.into(),
        ));
    }
    roots
}
fn retain_evidence(
    repo: &Path,
    fix: &str,
    target: &str,
    source: &str,
    tree: Option<&str>,
) -> Result<(), String> {
    for (reference, oid) in evidence_roots(fix, target, source, tree) {
        git::run(Some(repo), &["update-ref", &reference, &oid])?;
    }
    Ok(())
}
fn run(repo: &Path, plan: &Plan, stop: &git::RunStop) -> Result<Vec<Row>, String> {
    if !plan.trusted || plan.command.trim().is_empty() {
        return Err("Explicitly trust and enter the regression command first".into());
    }
    let fix = resolve(repo, &plan.fix)?;
    let targets = targets(repo, &plan.targets)?;
    let original_patch = patch(repo, &fix)?;
    let mut rows = vec![];
    for (target, tip, adaptation) in targets {
        let source = adaptation.as_ref().unwrap_or(&fix).clone();
        retain_evidence(repo, &fix, &tip, &source, None)?;
        let mut row = Row {
            target,
            tip: tip.clone(),
            original: fix.clone(),
            source: source.clone(),
            applied: None,
            error: None,
            test: None,
            adaptation: None,
            cleanup: None,
        };
        if stop.stopped() {
            row.error = Some("Cancelled before patch application; behavior not tested".into());
            rows.push(row);
            continue;
        }
        if adaptation.is_some() {
            row.adaptation = Some((original_patch.clone(), patch(repo, &source)?));
        }
        let scratch = git::Scratch::add_empty(repo, "rehearse", &tip)?;
        let result = (|| {
            scratch.checkout(&tip)?;
            git::run(
                Some(&scratch.path),
                &[
                    "-c",
                    "core.hooksPath=/dev/null",
                    "cherry-pick",
                    "--no-commit",
                    &source,
                ],
            )?;
            let tree = git::run(Some(&scratch.path), &["write-tree"])?
                .trim()
                .to_owned();
            // A ref directly to the tree is a real Git reachability root. JSON alone is not.
            retain_evidence(repo, &fix, &tip, &source, Some(&tree))?;
            row.applied = Some(tree.clone());
            if adaptation.is_some() {
                row.adaptation = Some((original_patch.clone(), patch(repo, &source)?));
            }
            if !stop.stopped() {
                row.test = Some(observe(&scratch.path, &tip, &tree, &plan.command, stop)?);
            }
            Ok::<_, String>(())
        })();
        if let Err(e) = result {
            row.error = Some(e);
        }
        row.cleanup = scratch.remove().err();
        rows.push(row);
    }
    let evidence = serde_json::to_string(&rows).map_err(|e| e.to_string())?;
    store::save(
        repo,
        "backport-contracts",
        &format!("evidence-{:x}", md5::compute(evidence)),
        &rows,
    )?;
    store::save(repo, "backport-contracts", "latest", &rows)?;
    Ok(rows)
}
impl RepoTab {
    pub(in crate::repo) fn contract_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Stop) {
            self.review_tools.contracts.stop.stop();
            return;
        }
        if self.review_tools.contracts.busy {
            return;
        }
        self.review_tools.contracts.busy = true;
        self.review_tools.contracts.stop = git::RunStop::default();
        let (repo, stop) = (self.path.clone(), self.review_tools.contracts.stop.clone());
        self.then_or(
            ctx,
            true,
            Err,
            move || match act {
                Act::Run(plan) => run(&repo, &plan, &stop),
                Act::Load => Ok(
                    store::load::<Vec<Row>>(&repo, "backport-contracts", "latest")?
                        .unwrap_or_default(),
                ),
                Act::Stop => Ok(vec![]),
            },
            |tab, result, _| {
                tab.review_tools.contracts.busy = false;
                tab.review_tools.contracts.rows = Some(result);
            },
        );
    }
}
pub(super) fn show(
    ui: &mut egui::Ui,
    s: &mut State,
    repo: &Path,
    branches: &[String],
    acts: &mut Vec<Action>,
) {
    ui.heading(crate::i18n::text(
        "repo-workflows-review-label-backport-by-contract-247835",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-drag-a-graph-commit-onto-a-release-below-or-en-775ed9",
    ));
    egui::CollapsingHeader::new(crate::i18n::text(
        "repo-workflows-review-label-drop-a-fix-onto-a-release-c1a96f",
    ))
    .id_salt("repo-workflows-review-label-drop-a-fix-onto-a-release-c1a96f")
    .show(ui, |ui| {
        for branch in branches {
            let response = ui.group(|ui| ui.label(branch)).response;
            if let Some(payload) = response.dnd_release_payload::<Payload>()
                && payload.repository() == repo
                && let DragItem::Commit(commit) = &payload.item
            {
                s.plan.fix = commit.hash.clone();
                s.plan.targets = branch.clone();
            }
        }
    });
    ui.add(
        egui::TextEdit::singleline(&mut s.plan.fix).hint_text(crate::i18n::text(
            "repo-workflows-review-label-fix-commit-c192f6",
        )),
    );
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-targets-one-per-line-optional-target-adaptatio-074e6e",
    ));
    ui.text_edit_multiline(&mut s.plan.targets);
    ui.add(
        egui::TextEdit::multiline(&mut s.plan.command).hint_text(crate::i18n::text(
            "repo-workflows-review-label-regression-command-e-g-cargo-test-token-lifeti-75b901",
        )),
    );
    ui.checkbox(
        &mut s.plan.trusted,
        crate::i18n::text(
            "repo-workflows-review-label-i-trust-this-command-and-the-repository-code-i-fcc4fd",
        ),
    );
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !s.busy && s.plan.trusted,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-review-label-rehearse-backports-and-regression-test-bf2331",
                )),
            )
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Contract(Act::Run(
                s.plan.clone(),
            ))));
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-load-preserved-results-23b59f",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Contract(Act::Load)));
        }
        if s.busy
            && ui
                .button(crate::i18n::text(
                    "repo-workflows-review-label-stop-rehearsal-1464d4",
                ))
                .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Contract(Act::Stop)));
        }
    });
    if let Some(rows) = &s.rows {
        match rows {
            Err(e) => {
                ui.label(crate::repo::messages::display_status(e));
            }
            Ok(rows) => {
                for row in rows {
                    ui.group(|ui| {
                        ui.strong(crate::i18n::message("repo-workflows-review-message-target-at-commit-9e4831", &[("target", (row.target.as_str()).into()), ("commit", (crate::repo::short(&row.tip)).into())]));
                        ui.label(if let Some(tree) = &row.applied {
                            crate::i18n::message("repo-workflows-review-message-patch-applied-staged-tree-tree-6e0863", &[("tree", (tree.as_str()).into())])
                        } else {
                            crate::i18n::text("repo-workflows-review-label-patch-did-not-apply-a1c90f").into()
                        });
                        if let Some(e) = &row.error {
                            ui.label(crate::repo::messages::display_status(e));
                        }
                        if let Some(test) = &row.test {
                            show_observation(ui, test);
                        } else {
                            ui.label(crate::i18n::text("repo-workflows-review-label-behavior-not-tested-9f5ef2"));
                        }
                        if let Some((original, adapted)) = &row.adaptation {
                            egui::CollapsingHeader::new(crate::i18n::text("repo-workflows-review-label-adaptation-next-to-original-df4874")).id_salt("repo-workflows-review-label-adaptation-next-to-original-df4874").show(
                                ui,
                                |ui| {
                                    ui.columns(2, |cols| {
                                        cols[0].label(crate::i18n::text("repo-workflows-review-label-original-fix-27d835"));
                                        cols[0].monospace(original);
                                        cols[1].label(crate::i18n::text("repo-workflows-review-label-adaptation-760fad"));
                                        cols[1].monospace(adapted);
                                    });
                                },
                            );
                        }
                        if let Some(e) = &row.cleanup {
                            ui.label(crate::i18n::message("repo-workflows-review-message-scratch-cleanup-requires-attention-detail-0f29d4", &[("detail", (crate::repo::messages::display_status(e)).into())]));
                        }
                    });
                }
            }
        }
    }
}
fn observation_summary(test: &Observation) -> String {
    if test.stopped {
        crate::i18n::message(
            "repo-workflows-review-message-test-stopped-command-command-machine-machine-t-03203f",
            &[
                ("command", (test.command.as_str()).into()),
                ("machine", (test.machine.as_str()).into()),
                ("tree", (test.tree.as_str()).into()),
            ],
        )
    } else if let Some(code) = test.code {
        crate::i18n::message(
            "repo-workflows-review-message-test-exit-code-command-command-machine-machine-96779e",
            &[
                ("code", (code).into()),
                ("command", (test.command.as_str()).into()),
                ("machine", (test.machine.as_str()).into()),
                ("tree", (test.tree.as_str()).into()),
            ],
        )
    } else {
        crate::i18n::message(
            "repo-workflows-review-message-test-exit-unknown-command-command-machine-mach-b250e6",
            &[
                ("command", (test.command.as_str()).into()),
                ("machine", (test.machine.as_str()).into()),
                ("tree", (test.tree.as_str()).into()),
            ],
        )
    }
}
pub(super) fn show_observation(ui: &mut egui::Ui, test: &Observation) {
    ui.label(observation_summary(test));
    egui::CollapsingHeader::new(crate::i18n::text(
        "repo-workflows-review-label-observed-output-49acbe",
    ))
    .id_salt("repo-workflows-review-label-observed-output-49acbe")
    .id_salt((&test.tree, &test.command))
    .show(ui, |ui| {
        ui.monospace(test.lines.join("\n"));
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rehearsal_separates_application_and_behavior_without_touching_checkout() -> Result<(), String>
    {
        let r = git::stack::test_repo::TestRepo::new("contract-rehearsal")?;
        let base = r.tip("HEAD")?;
        let fix = r.commit("fix.txt", "fixed\n", "fix")?;
        r.git(&["branch", "release", &base])?;
        r.branch("conflict", &base)?;
        r.commit("fix.txt", "different\n", "conflict")?;
        r.checkout("main")?;
        std::fs::write(r.path().join("base.txt"), "keep my dirty edit\n")
            .map_err(|e| e.to_string())?;
        let rows = run(
            r.path(),
            &Plan {
                fix: fix.clone(),
                targets: "release\nconflict".into(),
                command: "test -f fix.txt".into(),
                trusted: true,
            },
            &git::RunStop::default(),
        )?;
        let good = rows
            .iter()
            .find(|r| r.target == "release")
            .ok_or("missing release")?;
        assert!(good.applied.is_some());
        assert_eq!(good.test.as_ref().and_then(|t| t.code), Some(0));
        let conflict = rows
            .iter()
            .find(|r| r.target == "conflict")
            .ok_or("missing conflict")?;
        assert!(conflict.error.is_some());
        assert!(conflict.test.is_none());
        assert_eq!(r.tip("release")?, base);
        assert_eq!(r.tip("HEAD")?, fix);
        assert_eq!(
            std::fs::read_to_string(r.path().join("base.txt")).map_err(|e| e.to_string())?,
            "keep my dirty edit\n"
        );
        Ok(())
    }
    #[test]
    fn contract_targets_resolve_immutable_tips_and_explicit_adaptations() -> Result<(), String> {
        let r = git::stack::test_repo::TestRepo::new("contract-plan")?;
        let base = r.tip("HEAD")?;
        let fix = r.commit("fix.txt", "fixed\n", "fix")?;
        r.git(&["branch", "release", &base])?;
        let targets = targets(r.path(), &format!("release={fix}\nmain"))?;
        assert_eq!(targets.len(), 2);
        assert!(
            targets
                .iter()
                .any(|(n, t, a)| n == "release" && t == &base && a.as_ref() == Some(&fix))
        );
        assert!(self::targets(r.path(), "--help").is_err());
        assert!(self::targets(r.path(), "").is_err());
        let plan = Plan {
            fix,
            targets: "release".into(),
            command: "printf success".into(),
            trusted: false,
        };
        assert!(run(r.path(), &plan, &git::RunStop::default()).is_err());
        Ok(())
    }
    #[test]
    fn r2_pure_backport_roots_are_content_addressed_and_include_applied_tree() {
        let first = evidence_roots("fix-A", "release-A", "adaptation-A", Some("composite-A"));
        let second = evidence_roots("fix-B", "release-B", "adaptation-B", Some("composite-B"));
        assert!(first.iter().any(
            |(reference, oid)| reference.ends_with("trees/composite-A") && oid == "composite-A"
        ));
        assert!(
            first
                .iter()
                .all(|(name, _)| !second.iter().any(|(other, _)| other == name))
        );
        assert_eq!(first.len(), 4);
    }
    #[test]
    #[ignore = "R2 runtime hold: owned Git/GC fixture authored, not executed"]
    fn r2_runtime_backport_roots_survive_gc_after_source_rewrites() -> Result<(), String> {
        let r = git::stack::test_repo::TestRepo::new("backport-gc")?;
        let base = r.tip("HEAD")?;
        let fix = r.commit("fix.txt", "fix\n", "fix")?;
        r.git(&["reset", "--hard", &base])?;
        let target = r.commit("release-only.txt", "diverged\n", "release")?;
        r.git(&["cherry-pick", "--no-commit", &fix])?;
        let tree = git::run(Some(r.path()), &["write-tree"])?;
        retain_evidence(r.path(), &fix, &target, &fix, Some(tree.trim()))?;
        r.git(&["reset", "--hard", &base])?;
        let later = r.commit("later.txt", "later\n", "rewrite sources")?;
        retain_evidence(r.path(), &later, &base, &later, None)?;
        r.git(&["reflog", "expire", "--expire=now", "--all"])?;
        r.git(&["gc", "--prune=now"])?;
        r.git(&["cat-file", "-e", tree.trim()])?;
        r.git(&["cat-file", "-e", &fix])?;
        r.git(&["cat-file", "-e", &target])?;
        assert_eq!(
            r.git(&["show", &format!("{}:release-only.txt", tree.trim())])?,
            "diverged\n"
        );
        assert_eq!(
            r.git(&["show", &format!("{}:fix.txt", tree.trim())])?,
            "fix\n"
        );
        Ok(())
    }
}
