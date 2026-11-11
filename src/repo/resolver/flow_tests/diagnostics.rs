//! Failure-only snapshots: no extra polling, Git commands, waits or process control.
use super::*;

fn bounded(value: impl std::fmt::Debug) -> String {
    format!("{value:?}").chars().take(2048).collect()
}

pub(super) fn report(tab: Option<&RepoTab>) {
    let Some(tab) = tab else {
        eprintln!("resolver wait expired: no tab");
        return;
    };
    eprintln!(
        "resolver wait expired: opening={} busy={} status={} parked={} reading_parked={} read_again={} override_matches={} repo={}",
        tab.opening(),
        bounded(&tab.busy_op),
        bounded(tab.status.as_ref().map(|n| n.raw())),
        tab.resolver.parked.len(),
        tab.resolver.reading_parked,
        tab.resolver.read_parked_again,
        command_for(&tab.path).is_some(),
        bounded(&tab.path),
    );
    let Some(run) = &tab.resolver.run else {
        eprintln!("resolver run: absent");
        return;
    };
    eprintln!(
        "resolver run: id={} phase={:?} stopping={} ended={} parked={} error={} results={} last_words={}",
        run.id,
        run.phase,
        run.stopping,
        run.ended.is_some(),
        run.parked.is_some(),
        bounded(&run.error),
        bounded(&run.results),
        bounded(&run.last_words),
    );
    if let Some(agent) = &run.agent {
        agent.with_feed(|feed| {
            eprintln!(
                "resolver agent: exit={} activities={} recent={}",
                bounded(&feed.exit),
                feed.activities.len(),
                bounded(feed.activities.iter().rev().take(4).collect::<Vec<_>>())
            );
        });
    } else {
        eprintln!("resolver agent: absent");
    }
    if let Some(ws) = &run.workspace {
        eprintln!(
            "resolver workspace: dir={} kept={} exists={}",
            bounded(ws.dir()),
            ws.is_kept(),
            ws.dir().is_dir()
        );
    }
    // Flow fixtures are ordinary repositories; do not invoke Git to locate diagnostics.
    let dir = tab.path.join(".git/gitcraft/resolver").join(&run.id);
    for name in [
        "agent-archive-v1.json",
        "workspace.json",
        "run.json",
        "cleanup-unconfirmed",
    ] {
        let meta = std::fs::metadata(dir.join(name));
        eprintln!(
            "resolver archive: {name}={}",
            bounded(
                meta.map(|m| (m.is_file(), m.len()))
                    .map_err(|e| e.to_string())
            )
        );
    }
}
