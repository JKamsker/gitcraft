//! Patch identity, explicit adaptation, and observed behavior are independent witnesses.
use super::{
    contracts::{self, Observation},
    store,
    tools::ToolsAct,
};
use crate::{
    git,
    repo::{Action, RepoTab},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
const HISTORY: usize = 5000;
#[derive(Clone, Serialize, Deserialize)]
struct Pin {
    name: String,
    fix: String,
    targets: Vec<(String, Option<String>)>,
    command: String,
}
#[derive(Clone, Default)]
pub(crate) struct Form {
    name: String,
    fix: String,
    targets: String,
    command: String,
    trusted: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Row {
    name: String,
    fix: String,
    target: String,
    tip: String,
    patch: Patch,
    test: Option<Observation>,
    coverage: usize,
    complete: bool,
    error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
enum Patch {
    Same(String),
    Adapted(String),
    Reverted(String),
    NotChecked,
}
#[derive(Default)]
pub(crate) struct State {
    form: Form,
    pins: Vec<Pin>,
    rows: Option<Result<Vec<Row>, String>>,
    busy: bool,
    stop: git::RunStop,
}
impl Drop for State {
    fn drop(&mut self) {
        self.stop.stop();
    }
}
#[derive(Clone)]
pub(crate) enum Act {
    Load,
    Pin(Form),
    Check,
    Test { index: usize, trusted: bool },
    Stop,
    Remove(usize),
}
fn patch_id(repo: &Path, rev: &str, reverse: bool) -> Result<String, String> {
    let patch = if reverse {
        let parent =
            git::run(Some(repo), &["rev-parse", "--verify", &format!("{rev}^")]).or_else(|_| {
                git::run_input(Some(repo), &["hash-object", "-t", "tree", "--stdin"], "")
            })?;
        git::run(
            Some(repo),
            &[
                "diff",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                rev,
                parent.trim(),
                "--",
            ],
        )?
    } else {
        git::run(
            Some(repo),
            &[
                "show",
                "--format=medium",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                rev,
                "--",
            ],
        )?
    };
    let out = git::run_input(Some(repo), &["patch-id", "--stable"], &patch)?;
    out.split_whitespace()
        .next()
        .map(str::to_owned)
        .ok_or("Pin a nonempty, nonmerge fix patch".into())
}
fn history(repo: &Path, tip: &str) -> Result<(Vec<(String, String)>, usize, bool), String> {
    let limit = format!("--max-count={}", HISTORY + 1);
    let count = git::run(Some(repo), &["rev-list", "--no-merges", &limit, tip, "--"])?
        .lines()
        .count();
    let limit = format!("--max-count={HISTORY}");
    let patches = git::run(
        Some(repo),
        &[
            "log",
            "--no-merges",
            &limit,
            "--format=medium",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "-p",
            tip,
            "--",
        ],
    )?;
    let ids = git::run_input(Some(repo), &["patch-id", "--stable"], &patches)?
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            Some((words.next()?.into(), words.next()?.into()))
        })
        .collect();
    Ok((ids, count.min(HISTORY), count <= HISTORY))
}
fn status(
    ids: &[(String, String)],
    fix: &str,
    reverse: &str,
    adaptation: Option<&str>,
    repo: &Path,
    tip: &str,
) -> Patch {
    let found = ids.iter().position(|(id, _)| id == fix);
    let reverted = ids.iter().position(|(id, _)| id == reverse);
    if let Some(revert) = reverted
        && found.is_none_or(|fix| revert < fix)
    {
        return Patch::Reverted(ids[revert].1.clone());
    }
    if let Some(found) = found {
        return Patch::Same(ids[found].1.clone());
    }
    if let Some(adapted) = adaptation
        && git::is_ancestor(repo, adapted, tip)
    {
        return Patch::Adapted(adapted.into());
    }
    Patch::NotChecked
}
fn check(repo: &Path, pins: &[Pin]) -> Result<Vec<Row>, String> {
    let evidence = store::load::<BTreeMap<String, Observation>>(repo, "fix-tests", "results")?
        .unwrap_or_default();
    let mut rows = vec![];
    for pin in pins {
        let (id, reverse) = (
            patch_id(repo, &pin.fix, false)?,
            patch_id(repo, &pin.fix, true)?,
        );
        for (target, adaptation) in &pin.targets {
            let tip = contracts::resolve(repo, target)?;
            let (ids, coverage, complete) = history(repo, &tip)?;
            let tree = git::run(Some(repo), &["rev-parse", &format!("{tip}^{{tree}}")])?
                .trim()
                .to_owned();
            let key = evidence_key(&pin.fix, &tree, &pin.command);
            rows.push(Row {
                name: pin.name.clone(),
                fix: pin.fix.clone(),
                target: target.clone(),
                tip: tip.clone(),
                patch: status(&ids, &id, &reverse, adaptation.as_deref(), repo, &tip),
                test: evidence.get(&key).cloned(),
                coverage,
                complete,
                error: None,
            });
        }
    }
    Ok(rows)
}
fn evidence_key(fix: &str, tree: &str, command: &str) -> String {
    format!("{:x}", md5::compute(format!("{fix}\0{tree}\0{command}")))
}
fn test(repo: &Path, pin: &Pin, stop: &git::RunStop) -> Result<(), String> {
    if pin.command.trim().is_empty() {
        return Err("Give the pinned fix a regression command".into());
    }
    let mut evidence = store::load::<BTreeMap<String, Observation>>(repo, "fix-tests", "results")?
        .unwrap_or_default();
    for (target, _) in &pin.targets {
        if stop.stopped() {
            break;
        }
        let tip = contracts::resolve(repo, target)?;
        git::run(
            Some(repo),
            &[
                "update-ref",
                &format!("refs/gitcraft/fix-test-inputs/{}/{tip}", pin.fix),
                &tip,
            ],
        )?;
        let scratch = git::Scratch::add_empty(repo, "test", &tip)?;
        scratch.checkout(&tip)?;
        let tree = git::run(Some(&scratch.path), &["rev-parse", "HEAD^{tree}"])?
            .trim()
            .to_owned();
        let observation = contracts::observe(&scratch.path, &tip, &tree, &pin.command, stop)?;
        scratch.remove()?;
        // A stopped command remains explicitly stopped; it never becomes a passing witness.
        let key = evidence_key(&pin.fix, &tree, &pin.command);
        store::save(repo, "fix-test-evidence", &key, &observation)?;
        evidence.insert(key, observation);
        store::save(repo, "fix-tests", "results", &evidence)?;
    }
    Ok(())
}
impl RepoTab {
    pub(in crate::repo) fn radar_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Stop) {
            self.review_tools.radar.stop.stop();
            return;
        }
        if self.review_tools.radar.busy {
            return;
        }
        self.review_tools.radar.busy = true;
        self.review_tools.radar.stop = git::RunStop::default();
        let (repo, stop) = (self.path.clone(), self.review_tools.radar.stop.clone());
        self.then_or(
            ctx,
            true,
            Err,
            move || {
                let mut pins =
                    store::load::<Vec<Pin>>(&repo, "fix-radar", "pins")?.unwrap_or_default();
                match act {
                    Act::Pin(form) => {
                        let fix = contracts::resolve(&repo, &form.fix)?;
                        patch_id(&repo, &fix, false)?;
                        let mut targets = vec![];
                        for line in form
                            .targets
                            .lines()
                            .map(str::trim)
                            .filter(|l| !l.is_empty())
                        {
                            let (target, adaptation) = line
                                .split_once('=')
                                .map_or((line, None), |(t, a)| (t.trim(), Some(a.trim())));
                            contracts::resolve(&repo, target)?;
                            targets.push((
                                target.into(),
                                adaptation
                                    .map(|a| contracts::resolve(&repo, a))
                                    .transpose()?,
                            ));
                        }
                        if targets.is_empty() || targets.len() > 32 {
                            return Err("Choose 1–32 release targets".into());
                        }
                        git::run(
                            Some(&repo),
                            &[
                                "update-ref",
                                &format!("refs/gitcraft/pinned-fixes/{fix}"),
                                &fix,
                            ],
                        )?;
                        for (_, adapted) in &targets {
                            if let Some(adapted) = adapted {
                                git::run(
                                    Some(&repo),
                                    &[
                                        "update-ref",
                                        &format!("refs/gitcraft/pinned-fixes/{adapted}"),
                                        adapted,
                                    ],
                                )?;
                            }
                        }
                        pins.push(Pin {
                            name: form.name,
                            fix,
                            targets,
                            command: form.command,
                        });
                        store::save(&repo, "fix-radar", "pins", &pins)?;
                    }
                    Act::Remove(index) => {
                        if index >= pins.len() {
                            return Err("Reload the pinned fixes".into());
                        }
                        pins.remove(index);
                        store::save(&repo, "fix-radar", "pins", &pins)?;
                    }
                    Act::Test { index, trusted } => {
                        if !trusted {
                            return Err(
                                "Explicitly trust the regression command before running it".into(),
                            );
                        }
                        test(&repo, pins.get(index).ok_or("Reload pinned fixes")?, &stop)?;
                    }
                    _ => {}
                }
                let rows = check(&repo, &pins)?;
                Ok((pins, rows))
            },
            |tab, result, _| {
                let s = &mut tab.review_tools.radar;
                s.busy = false;
                match result {
                    Ok((pins, rows)) => {
                        s.pins = pins;
                        s.rows = Some(Ok(rows));
                    }
                    Err(e) => s.rows = Some(Err(e)),
                }
            },
        );
    }
}
pub(super) fn show(ui: &mut egui::Ui, s: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Fix survival radar");
    ui.label("Patch identity and tested behavior are independent. Not found in inspected history stays not checked. Reverse-patch evidence makes the branch amber.");
    ui.horizontal(|ui| {
        for (label, act) in [
            ("Load pinned fixes", Act::Load),
            ("Refresh branch observations", Act::Check),
        ] {
            if ui.add_enabled(!s.busy, egui::Button::new(label)).clicked() {
                acts.push(Action::ReviewTools(ToolsAct::Radar(act)));
            }
        }
        if s.busy && ui.button("Stop regression checks").clicked() {
            acts.push(Action::ReviewTools(ToolsAct::Radar(Act::Stop)));
        }
    });
    for (i, pin) in s.pins.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "{} · {} · {}",
                pin.name,
                crate::repo::short(&pin.fix),
                pin.command
            ));
            if ui
                .add_enabled(
                    !s.busy && s.form.trusted,
                    egui::Button::new("Test target behaviors"),
                )
                .clicked()
            {
                acts.push(Action::ReviewTools(ToolsAct::Radar(Act::Test {
                    index: i,
                    trusted: s.form.trusted,
                })));
            }
            if ui.small_button("Unpin").clicked() {
                acts.push(Action::ReviewTools(ToolsAct::Radar(Act::Remove(i))));
            }
        });
    }
    if let Some(rows) = &s.rows {
        match rows {
            Err(e) => {
                ui.label(e);
            }
            Ok(rows) => {
                for row in rows {
                    ui.group(|ui| {
                        ui.strong(format!(
                            "{} · {} at {}",
                            row.name,
                            row.target,
                            crate::repo::short(&row.tip)
                        ));
                        match &row.patch {
                            Patch::Same(commit) => {
                                ui.label(format!("Same patch found: {commit}"));
                            }
                            Patch::Adapted(commit) => {
                                ui.label(format!("Explicit adaptation linked: {commit}"));
                                if ui.button("Compare adaptation with original").clicked() {
                                    acts.push(Action::Compare {
                                        base: row.fix.clone(),
                                        base_label: "Original fix".into(),
                                        target: Some(commit.clone()),
                                        target_label: "Adapted fix".into(),
                                        since_fork: false,
                                    });
                                }
                            }
                            Patch::Reverted(commit) => {
                                ui.colored_label(
                                    egui::Color32::YELLOW,
                                    format!("Amber: later reverse patch at {commit}"),
                                );
                            }
                            Patch::NotChecked => {
                                ui.label("Patch not checked / no match in inspected history");
                            }
                        }
                        ui.weak(format!(
                            "{} nonmerge commits inspected · {}",
                            row.coverage,
                            if row.complete {
                                "complete reachable nonmerge history"
                            } else {
                                "older history not inspected"
                            }
                        ));
                        if let Some(test) = &row.test {
                            contracts::show_observation(ui, test);
                        } else {
                            ui.label("Regression behavior not checked for this tree and command");
                        }
                    });
                }
            }
        }
    }
    ui.separator();
    ui.add(egui::TextEdit::singleline(&mut s.form.name).hint_text("Fix name"));
    ui.add(egui::TextEdit::singleline(&mut s.form.fix).hint_text("Fix commit"));
    ui.label("Release refs, optionally release=adapted-commit, one per line:");
    ui.text_edit_multiline(&mut s.form.targets);
    ui.add(
        egui::TextEdit::singleline(&mut s.form.command)
            .hint_text("Fix-specific regression command"),
    );
    ui.checkbox(
        &mut s.form.trusted,
        "I trust running these regression commands and repository code in scratch worktrees",
    );
    if ui
        .add_enabled(!s.busy, egui::Button::new("Pin this fix"))
        .clicked()
    {
        acts.push(Action::ReviewTools(ToolsAct::Radar(Act::Pin(
            s.form.clone(),
        ))));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radar_detects_equivalent_patch_then_revert_without_claiming_behavior() -> Result<(), String>
    {
        let r = git::stack::test_repo::TestRepo::new("fix-radar")?;
        let base = r.tip("HEAD")?;
        let fix = r.commit("fix.txt", "fixed\n", "fix")?;
        let id = patch_id(r.path(), &fix, false)?;
        let reverse = patch_id(r.path(), &fix, true)?;
        r.branch("release", &base)?;
        r.git(&["cherry-pick", &fix])?;
        let tip = r.tip("HEAD")?;
        let (ids, _, complete) = history(r.path(), &tip)?;
        assert!(complete);
        assert!(matches!(
            status(&ids, &id, &reverse, None, r.path(), &tip),
            Patch::Same(_)
        ));
        r.git(&["revert", "--no-edit", &tip])?;
        let reverted = r.tip("HEAD")?;
        let (ids, _, _) = history(r.path(), &reverted)?;
        assert_eq!(
            status(&ids, &id, &reverse, None, r.path(), &reverted),
            Patch::Reverted(reverted.clone())
        );
        r.git(&["revert", "--no-edit", &reverted])?;
        let restored = r.tip("HEAD")?;
        let (ids, _, _) = history(r.path(), &restored)?;
        assert!(matches!(
            status(&ids, &id, &reverse, None, r.path(), &restored),
            Patch::Same(_)
        ));
        Ok(())
    }
}
