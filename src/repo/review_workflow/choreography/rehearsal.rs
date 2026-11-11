//! Rehearsal responsibilities for choreography.
use super::*;

pub(super) fn rehearse_state(
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
pub(super) fn run(
    repo: &Path,
    plan: &Plan,
    tested: bool,
    stop: &git::RunStop,
) -> Result<Report, String> {
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
