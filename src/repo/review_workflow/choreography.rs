//! Cross-repository release planning and compatibility rehearsal. No publishing operations.
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
use std::{
    collections::{BTreeSet, VecDeque},
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize, Default)]
struct Component {
    name: String,
    repo: String,
    revisions: String,
}
#[derive(Clone, Serialize, Deserialize, Default)]
struct Rule {
    dependent: String,
    at: usize,
    prerequisite: String,
    min: usize,
    reason: String,
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Plan {
    components: Vec<Component>,
    rules: Vec<Rule>,
    command: String,
    trusted: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Stage {
    label: String,
    commit: String,
    tree: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Source {
    name: String,
    repo: PathBuf,
    stages: Vec<Stage>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Combination {
    stages: Vec<usize>,
    allowed: bool,
    observed: Option<Observation>,
    error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Report {
    sources: Vec<Source>,
    rules: Vec<Rule>,
    command: String,
    states: Vec<Combination>,
    route: Option<Vec<Vec<usize>>>,
    tested: bool,
    stopped: bool,
    cleanup: Vec<String>,
}
#[derive(Default)]
pub(crate) struct State {
    plan: Plan,
    busy: bool,
    stop: git::RunStop,
    report: Option<Report>,
    error: Option<String>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.stop.stop();
    }
}
#[derive(Clone)]
pub(crate) enum Act {
    Save(Plan),
    Load,
    Plan(Plan),
    Rehearse(Plan),
    Stop,
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn resolve(repo: &Path, plan: &Plan) -> Result<Vec<Source>, String> {
    if !(2..=6).contains(&plan.components.len()) {
        return Err("Add 2–6 repositories to the release board".into());
    }
    let machine = crate::remote::parse(repo).map(|t| t.host);
    let mut names = BTreeSet::new();
    let mut sources = vec![];
    let mut combinations = 1usize;
    for component in &plan.components {
        if !valid_name(&component.name) || !names.insert(component.name.to_ascii_uppercase()) {
            return Err("Component names must be unique ASCII identifiers, ignoring case".into());
        }
        let path = PathBuf::from(component.repo.trim());
        if !path.is_absolute() && !crate::remote::is_remote(&path) {
            return Err("Use absolute repository paths or encoded SSH/WSL paths".into());
        }
        if path.as_os_str().is_empty() {
            return Err(format!("Choose the repository for {}", component.name));
        }
        if crate::remote::parse(&path).map(|t| t.host) != machine {
            return Err("Rehearse linked repositories on the same machine; choose local copies or paths on one shared SSH/WSL host".into());
        }
        let mut stages = vec![];
        for (i, line) in component
            .revisions
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .enumerate()
        {
            let (label, revision) = line
                .split_once('=')
                .map_or((format!("Stage {i}"), line), |(a, b)| {
                    (a.trim().into(), b.trim())
                });
            let commit = contracts::resolve(&path, revision)?;
            let tree = git::run(Some(&path), &["rev-parse", &format!("{commit}^{{tree}}")])?
                .trim()
                .to_owned();
            stages.push(Stage {
                label,
                commit,
                tree,
            });
        }
        if !(2..=4).contains(&stages.len()) {
            return Err(format!(
                "{} needs 2–4 ordered stages: deployed, optional bridge, next",
                component.name
            ));
        }
        combinations = combinations
            .checked_mul(stages.len())
            .ok_or("Too many combinations")?;
        if combinations > 256 {
            return Err(
                "More than 256 in-between combinations: split this release into smaller boards"
                    .into(),
            );
        }
        sources.push(Source {
            name: component.name.clone(),
            repo: path,
            stages,
        });
    }
    for rule in &plan.rules {
        let dependent = sources
            .iter()
            .find(|s| s.name == rule.dependent)
            .ok_or("A rule names an unknown dependent component")?;
        let prerequisite = sources
            .iter()
            .find(|s| s.name == rule.prerequisite)
            .ok_or("A rule names an unknown prerequisite component")?;
        if rule.at == 0
            || rule.at >= dependent.stages.len()
            || rule.min == 0
            || rule.min >= prerequisite.stages.len()
        {
            return Err(
                "Rule stage thresholds must name non-deployed stages present on their components"
                    .into(),
            );
        }
    }
    Ok(sources)
}
fn allowed(state: &[usize], sources: &[Source], rules: &[Rule]) -> bool {
    rules.iter().all(|r| {
        let a = sources.iter().position(|s| s.name == r.dependent);
        let b = sources.iter().position(|s| s.name == r.prerequisite);
        match (a, b) {
            (Some(a), Some(b)) => state[a] < r.at || state[b] >= r.min,
            _ => false,
        }
    })
}
fn combinations(sources: &[Source], rules: &[Rule]) -> Vec<Combination> {
    let mut states = vec![vec![]];
    for source in sources {
        states = states
            .into_iter()
            .flat_map(|prefix| {
                (0..source.stages.len()).map(move |stage| {
                    let mut next = prefix.clone();
                    next.push(stage);
                    next
                })
            })
            .collect();
    }
    states
        .into_iter()
        .map(|stages| Combination {
            allowed: allowed(&stages, sources, rules),
            stages,
            observed: None,
            error: None,
        })
        .collect()
}
fn route(states: &[Combination], sources: &[Source], tested: bool) -> Option<Vec<Vec<usize>>> {
    let good: BTreeSet<_> = states
        .iter()
        .filter(|s| {
            s.allowed
                && s.error.is_none()
                && (!tested
                    || s.observed
                        .as_ref()
                        .is_some_and(|o| o.code == Some(0) && !o.stopped))
        })
        .map(|s| s.stages.clone())
        .collect();
    let start = vec![0; sources.len()];
    let goal: Vec<_> = sources.iter().map(|s| s.stages.len() - 1).collect();
    if !good.contains(&start) || !good.contains(&goal) {
        return None;
    }
    let mut queue = VecDeque::from([vec![start.clone()]]);
    let mut seen = BTreeSet::from([start]);
    while let Some(path) = queue.pop_front() {
        let at = path.last()?;
        if at == &goal {
            return Some(path);
        }
        for i in 0..sources.len() {
            let mut next = at.clone();
            next[i] += 1;
            if next[i] < sources[i].stages.len()
                && good.contains(&next)
                && seen.insert(next.clone())
            {
                let mut path = path.clone();
                path.push(next);
                queue.push_back(path);
            }
        }
    }
    None
}
fn rehearse_state(
    sources: &[Source],
    state: &[usize],
    command: &str,
    stop: &git::RunStop,
) -> Result<(Observation, Vec<String>), String> {
    let mut scratches = vec![];
    let mut vars = vec![];
    for (source, &stage) in sources.iter().zip(state) {
        if stop.stopped() {
            return Err("Stopped before constructing the full combination".into());
        }
        let scratch =
            git::Scratch::add_empty(&source.repo, "rehearse", &source.stages[stage].commit)?;
        scratch.checkout(&source.stages[stage].commit)?;
        let name = source.name.to_ascii_uppercase();
        vars.push((
            format!("GITCRAFT_COMPONENT_{name}"),
            crate::remote::git_path(&scratch.path),
        ));
        vars.push((format!("GITCRAFT_STAGE_{name}"), stage.to_string()));
        scratches.push(scratch);
    }
    let script = crate::custom_commands::captured_program(&vars, command);
    let digest = format!(
        "{:x}",
        md5::compute(
            sources
                .iter()
                .zip(state)
                .map(|(s, &i)| format!("{}:{}", s.name, s.stages[i].tree))
                .collect::<Vec<_>>()
                .join("\n")
        )
    );
    let result = contracts::observe(
        &scratches[0].path,
        &sources[0].stages[state[0]].commit,
        &digest,
        &script,
        stop,
    );
    let cleanup = scratches
        .into_iter()
        .filter_map(|s| s.remove().err())
        .collect();
    let mut observation = result?;
    observation.command = command.into();
    Ok((observation, cleanup))
}
fn run(repo: &Path, plan: &Plan, tested: bool, stop: &git::RunStop) -> Result<Report, String> {
    if tested && (!plan.trusted || plan.command.trim().is_empty()) {
        return Err(
            "Enter and explicitly trust your compatibility command before rehearsal".into(),
        );
    }
    let sources = resolve(repo, plan)?;
    let mut states = combinations(&sources, &plan.rules);
    let mut cleanup = vec![];
    if tested {
        for source in &sources {
            for (i, stage) in source.stages.iter().enumerate() {
                git::run(
                    Some(&source.repo),
                    &[
                        "update-ref",
                        &format!(
                            "refs/gitcraft/release-inputs/{:x}/{}/{i}",
                            md5::compute(serde_json::to_string(plan).map_err(|e| e.to_string())?),
                            source.name
                        ),
                        &stage.commit,
                    ],
                )?;
            }
        }
        for state in &mut states {
            if !state.allowed {
                continue;
            }
            if stop.stopped() {
                state.error = Some("Not tested: rehearsal stopped".into());
                continue;
            }
            match rehearse_state(&sources, &state.stages, &plan.command, stop) {
                Ok((observed, left)) => {
                    state.observed = Some(observed);
                    cleanup.extend(left);
                }
                Err(e) => state.error = Some(e),
            }
        }
    }
    let report = Report {
        route: route(&states, &sources, tested),
        sources,
        rules: plan.rules.clone(),
        command: plan.command.clone(),
        states,
        tested,
        stopped: stop.stopped(),
        cleanup,
    };
    let text = serde_json::to_string(&report).map_err(|e| e.to_string())?;
    store::save(
        repo,
        "release-rehearsal-evidence",
        &format!("{:x}", md5::compute(text)),
        &report,
    )?;
    store::save(repo, "release-rehearsal-evidence", "latest", &report)?;
    Ok(report)
}
impl RepoTab {
    pub(in crate::repo) fn choreography_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Stop) {
            self.review_tools.choreography.stop.stop();
            return;
        }
        if self.review_tools.choreography.busy {
            return;
        }
        self.review_tools.choreography.busy = true;
        self.review_tools.choreography.stop = git::RunStop::default();
        let (repo, stop) = (
            self.path.clone(),
            self.review_tools.choreography.stop.clone(),
        );
        self.then_or(
            ctx,
            true,
            Err,
            move || match act {
                Act::Save(plan) => {
                    store::save(&repo, "release-board", "plan", &plan)?;
                    Ok((Some(plan), None))
                }
                Act::Load => Ok((
                    store::load::<Plan>(&repo, "release-board", "plan")?,
                    store::load::<Report>(&repo, "release-rehearsal-evidence", "latest")?,
                )),
                Act::Plan(plan) => run(&repo, &plan, false, &stop).map(|r| (None, Some(r))),
                Act::Rehearse(plan) => run(&repo, &plan, true, &stop).map(|r| (None, Some(r))),
                Act::Stop => Ok((None, None)),
            },
            |tab, result, _| {
                let s = &mut tab.review_tools.choreography;
                s.busy = false;
                match result {
                    Ok((plan, report)) => {
                        s.error = None;
                        if let Some(plan) = plan {
                            s.plan = plan;
                        }
                        if report.is_some() {
                            s.report = report;
                        }
                    }
                    Err(e) => s.error = Some(e),
                }
            },
        );
    }
}
pub(super) fn show(ui: &mut egui::Ui, s: &mut State, repo: &Path, acts: &mut Vec<Action>) {
    ui.heading("Release choreography across repositories");
    ui.label("Give each repository deployed, optional bridge, and next revisions. Rules constrain when a component may switch. Rehearsal tests every permitted in-between combination and searches for a passing release order. Publishing is a separate operation.");
    let mut remove = None;
    for (i, component) in s.plan.components.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Component {}", i + 1));
                if ui
                    .small_button(crate::i18n::text(
                        "repo-workflows-review_workflow-show-c3812fc4",
                    ))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut component.name)
                    .hint_text("Identifier, e.g. backend"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut component.repo)
                    .hint_text("Repository path (same machine as this board)"),
            );
            ui.label("Ordered stages, one label=revision per line (first is deployed):");
            ui.text_edit_multiline(&mut component.revisions);
        });
    }
    if let Some(i) = remove {
        s.plan.components.remove(i);
    }
    if ui.button("Add repository component").clicked() {
        s.plan.components.push(Component {
            repo: repo.to_string_lossy().into(),
            ..Default::default()
        });
    }
    let names: Vec<_> = s.plan.components.iter().map(|c| c.name.clone()).collect();
    let mut remove = None;
    for (i, rule) in s.plan.rules.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("When");
                egui::ComboBox::from_id_salt(("rule-dependent", i))
                    .selected_text(&rule.dependent)
                    .show_ui(ui, |ui| {
                        for n in &names {
                            ui.selectable_value(&mut rule.dependent, n.clone(), n);
                        }
                    });
                ui.label("reaches stage");
                ui.add(egui::DragValue::new(&mut rule.at).range(1..=3));
                ui.label("require");
                egui::ComboBox::from_id_salt(("rule-prerequisite", i))
                    .selected_text(&rule.prerequisite)
                    .show_ui(ui, |ui| {
                        for n in &names {
                            ui.selectable_value(&mut rule.prerequisite, n.clone(), n);
                        }
                    });
                ui.label("at least stage");
                ui.add(egui::DragValue::new(&mut rule.min).range(1..=3));
                if ui.small_button("Remove rule").clicked() {
                    remove = Some(i);
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut rule.reason)
                    .hint_text("e.g. backend supports both protocols before SDK switches"),
            );
        });
    }
    if let Some(i) = remove {
        s.plan.rules.remove(i);
    }
    if ui.button("Add ordering / compatibility rule").clicked() {
        s.plan.rules.push(Rule {
            at: 1,
            min: 1,
            ..Default::default()
        });
    }
    ui.add(egui::TextEdit::multiline(&mut s.plan.command).hint_text("Compatibility command"));
    ui.weak("The command receives GITCRAFT_COMPONENT_<NAME> scratch paths and GITCRAFT_STAGE_<NAME> numbers. Run cross-project integration tests using those paths. Each combination starts in fresh detached worktrees.");
    ui.checkbox(
        &mut s.plan.trusted,
        "I trust this compatibility command and the repository code it runs",
    );
    ui.horizontal_wrapped(|ui| {
        for (label, act) in [
            ("Save board", Act::Save(s.plan.clone())),
            ("Load board and evidence", Act::Load),
            (
                "Plan order without running tests",
                Act::Plan(s.plan.clone()),
            ),
            (
                "Rehearse every permitted combination",
                Act::Rehearse(s.plan.clone()),
            ),
        ] {
            if ui.add_enabled(!s.busy, egui::Button::new(label)).clicked() {
                acts.push(Action::ReviewTools(ToolsAct::Choreography(act)));
            }
        }
        if s.busy && ui.button("Stop rehearsal").clicked() {
            acts.push(Action::ReviewTools(ToolsAct::Choreography(Act::Stop)));
        }
    });
    if let Some(e) = &s.error {
        ui.label(e);
    }
    if let Some(r) = &s.report {
        ui.label(if r.tested {
            "Observed compatibility under your recorded command"
        } else {
            "Candidate plan only — no compatibility test executed"
        });
        if r.stopped {
            ui.label("Stopped: untested combinations cannot establish a passing order");
        }
        if let Some(route) = &r.route {
            for pair in route.windows(2) {
                if let Some(i) = pair[0].iter().zip(&pair[1]).position(|(a, b)| a != b) {
                    let stage = &r.sources[i].stages[pair[1][i]];
                    ui.label(format!(
                        "{} → {} ({})",
                        r.sources[i].name,
                        stage.label,
                        crate::repo::short(&stage.commit)
                    ));
                }
            }
        } else {
            ui.label("No permitted passing release order was established");
        }
        egui::CollapsingHeader::new(format!(
            "{} combinations and their evidence",
            r.states.len()
        ))
        .show(ui, |ui| {
            for combination in &r.states {
                ui.group(|ui| {
                    ui.monospace(
                        r.sources
                            .iter()
                            .zip(&combination.stages)
                            .map(|(source, &i)| {
                                format!(
                                    "{}={} ({})",
                                    source.name,
                                    source.stages[i].label,
                                    crate::repo::short(&source.stages[i].commit)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" · "),
                    );
                    if !combination.allowed {
                        ui.label("Excluded by your declared ordering rules");
                    } else if let Some(error) = &combination.error {
                        ui.label(error);
                    } else if let Some(observed) = &combination.observed {
                        contracts::show_observation(ui, observed);
                    } else {
                        ui.label("Not tested");
                    }
                });
            }
        });
        for e in &r.cleanup {
            ui.label(format!("Scratch cleanup needs attention: {e}"));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sources() -> Vec<Source> {
        ["backend", "sdk"]
            .iter()
            .map(|name| Source {
                name: (*name).into(),
                repo: PathBuf::from("/recording"),
                stages: (0..2)
                    .map(|i| Stage {
                        label: i.to_string(),
                        commit: i.to_string(),
                        tree: i.to_string(),
                    })
                    .collect(),
            })
            .collect()
    }
    #[test]
    fn rehearsed_cross_repo_behavior_derives_a_passing_order() -> Result<(), String> {
        let backend = git::stack::test_repo::TestRepo::new("release-backend")?;
        let sdk = git::stack::test_repo::TestRepo::new("release-sdk")?;
        let b0 = backend.commit("backend.txt", "old\n", "deployed")?;
        let b1 = backend.commit("backend.txt", "dual\n", "bridge")?;
        let s0 = sdk.commit("sdk.txt", "old\n", "deployed")?;
        let s1 = sdk.commit("sdk.txt", "new\n", "switch")?;
        let plan=Plan{components:vec![
            Component{name:"backend".into(),repo:backend.path().to_string_lossy().into(),revisions:format!("deployed={b0}\nbridge={b1}")},
            Component{name:"sdk".into(),repo:sdk.path().to_string_lossy().into(),revisions:format!("deployed={s0}\nnext={s1}")}
        ],rules:vec![],command:r#"test "$(cat "$GITCRAFT_COMPONENT_SDK/sdk.txt")" != new || test "$(cat "$GITCRAFT_COMPONENT_BACKEND/backend.txt")" = dual"#.into(),trusted:true};
        let report = run(backend.path(), &plan, true, &git::RunStop::default())?;
        assert_eq!(report.states.len(), 4);
        assert_eq!(
            report
                .states
                .iter()
                .filter(|s| s.observed.as_ref().is_some_and(|o| o.code == Some(1)))
                .count(),
            1
        );
        assert_eq!(report.route, Some(vec![vec![0, 0], vec![1, 0], vec![1, 1]]));
        assert_eq!(backend.tip("HEAD")?, b1);
        assert_eq!(sdk.tip("HEAD")?, s1);
        assert!(report.cleanup.is_empty());
        Ok(())
    }
    #[test]
    fn ordering_rules_and_observed_failures_control_the_route() {
        let sources = sources();
        let rules = vec![Rule {
            dependent: "sdk".into(),
            at: 1,
            prerequisite: "backend".into(),
            min: 1,
            reason: "dual protocol first".into(),
        }];
        let mut states = combinations(&sources, &rules);
        assert_eq!(
            route(&states, &sources, false),
            Some(vec![vec![0, 0], vec![1, 0], vec![1, 1]])
        );
        assert!(route(&states, &sources, true).is_none());
        for s in &mut states {
            if s.allowed {
                s.observed = Some(Observation {
                    commit: "pinned".into(),
                    tree: "combination".into(),
                    command: "integration".into(),
                    code: Some(if s.stages == [1, 0] { 1 } else { 0 }),
                    stopped: false,
                    lines: vec![],
                    machine: "recording".into(),
                });
            }
        }
        assert!(route(&states, &sources, true).is_none());
        if let Some(state) = states.iter_mut().find(|s| s.stages == [1, 0])
            && let Some(o) = &mut state.observed
        {
            o.code = Some(0);
        }
        assert!(route(&states, &sources, true).is_some());
    }
    #[test]
    fn command_environment_names_cannot_inject_shell_syntax() {
        assert!(valid_name("backend_v2"));
        for bad in ["", "2sdk", "SDK;rm", "$(command)", "SDK-NEXT"] {
            assert!(!valid_name(bad));
        }
    }
}
