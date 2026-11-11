//! Forge observations attach to the already assembled local branches and tips.
use super::*;

pub(super) fn pull_requests(m: &mut Model, forge: &Forge) {
    match &forge.prs {
        Some(prs) => {
            for p in prs {
                let Some(b) = m.find(Kind::Branch, &p.branch).filter(|_| !p.from_fork) else {
                    continue;
                };
                let tip = m.nodes[b].commit.clone().unwrap_or_default();
                let mut detail = vec![p.title.clone(), format!("{} into {}", p.branch, p.base)];
                if p.sha != tip {
                    detail.push(format!(
                        "its head {} is not the local tip {}",
                        short(&p.sha),
                        short(&tip)
                    ));
                }
                let flags = if p.draft {
                    vec![(Flag::Draft, Evidence::Observed)]
                } else {
                    Vec::new()
                };
                let i = m.nodes.len();
                m.nodes.push(Node {
                    kind: Kind::Pr,
                    key: p.number.to_string(),
                    label: format!("#{}", p.number),
                    flags,
                    detail,
                    commit: Some(p.sha.clone()),
                });
                m.edges.push(Edge {
                    from: b,
                    to: i,
                    link: Link::Proposes,
                    evidence: Evidence::Observed,
                    note: "the pull request's head branch".to_owned(),
                });
            }
        }
        None => m
            .notes
            .push("Pull requests: not read (no forge connected, or not loaded yet)".to_owned()),
    }
}

pub(super) fn builds(m: &mut Model, forge: &Forge) {
    let builds = forge.builds.as_ref();
    match builds {
        Some(builds) => {
            // The commits built: branch tips and pull request heads, each once.
            let sources: Vec<(usize, String)> = m
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| matches!(n.kind, Kind::Branch | Kind::Pr))
                .filter_map(|(i, n)| n.commit.clone().map(|c| (i, c)))
                .collect();
            for (from, sha) in sources {
                let Some(&state) = builds.get(&sha) else {
                    continue;
                };
                let i = match m.find(Kind::Build, &sha) {
                    Some(i) => i,
                    None => {
                        m.nodes.push(Node {
                            kind: Kind::Build,
                            key: sha.clone(),
                            label: format!("{} {}", short(&sha), state_word(state)),
                            flags: build_flag(state)
                                .map(|f| vec![(f, Evidence::Observed)])
                                .unwrap_or_default(),
                            detail: vec![format!(
                                "builds of {}: {}",
                                short(&sha),
                                state_word(state)
                            )],
                            commit: Some(sha.clone()),
                        });
                        m.nodes.len() - 1
                    }
                };
                m.edges.push(Edge {
                    from,
                    to: i,
                    link: Link::Built,
                    evidence: Evidence::Observed,
                    note: "the forge's checks of that commit".to_owned(),
                });
            }
            m.notes.push(
                "Commits without builds have none the forge reported: not known, not failing"
                    .to_owned(),
            );
        }
        None => m
            .notes
            .push("Builds: not read (no forge with CI connected, or not loaded yet)".to_owned()),
    }
}
