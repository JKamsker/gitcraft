//! Planning responsibilities for choreography.
use super::*;

pub(super) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
pub(super) fn stage_input(line: &str, i: usize) -> (String, &str, Option<usize>) {
    line.split_once('=')
        .map_or((format!("Stage {i}"), line, Some(i)), |(a, b)| {
            (a.trim().into(), b.trim(), None)
        })
}
pub(super) fn display_stage(stage: &Stage) -> String {
    match stage.generated_index {
        Some(stage) => crate::i18n::message(
            "repo-workflows-review-raw-stage-i-53efae",
            &[("stage", stage.into())],
        ),
        None => stage.label.clone(),
    }
}
pub(super) fn resolve(repo: &Path, plan: &Plan) -> Result<Vec<Source>, String> {
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
            let (label, revision, generated_index) = stage_input(line, i);
            let commit = contracts::resolve(&path, revision)?;
            let tree = git::run(Some(&path), &["rev-parse", &format!("{commit}^{{tree}}")])?
                .trim()
                .to_owned();
            stages.push(Stage {
                label,
                commit,
                tree,
                generated_index,
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
pub(super) fn allowed(state: &[usize], sources: &[Source], rules: &[Rule]) -> bool {
    rules.iter().all(|r| {
        let a = sources.iter().position(|s| s.name == r.dependent);
        let b = sources.iter().position(|s| s.name == r.prerequisite);
        match (a, b) {
            (Some(a), Some(b)) => state[a] < r.at || state[b] >= r.min,
            _ => false,
        }
    })
}
pub(super) fn combinations(sources: &[Source], rules: &[Rule]) -> Vec<Combination> {
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
pub(super) fn route(
    states: &[Combination],
    sources: &[Source],
    tested: bool,
) -> Option<Vec<Vec<usize>>> {
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
