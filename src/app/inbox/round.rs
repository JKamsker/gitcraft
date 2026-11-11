//! Pure inbox round grouping, deduplication, and seen-state reconciliation.

use super::*;

pub(super) fn asks(found: Vec<Hosted>) -> Vec<Ask> {
    let mut asks: Vec<Ask> = Vec::new();
    for (repo, hosting) in found {
        let per_host = hosting.forge.provider().inbox_per_host();
        let same = asks.iter_mut().find(|a| {
            a.hosting.forge == hosting.forge
                && a.hosting.host == hosting.host
                && (per_host || a.hosting.path == hosting.path)
        });
        match same {
            Some(ask) => ask.clones.push((repo, hosting)),
            None => asks.push(Ask {
                repo: repo.clone(),
                hosting: hosting.clone(),
                clones: vec![(repo, hosting)],
            }),
        }
    }
    asks
}

/// Each pull request once, the most recently changed first.
pub(super) fn dedupe(items: Vec<Item>) -> Vec<Item> {
    let mut seen = HashSet::new();
    let mut items: Vec<Item> = items
        .into_iter()
        .filter(|i| seen.insert(i.pr.url.clone()))
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.pr.updated));
    items
}

/// The items that need you now, with why, given what was seen of them.
pub(in crate::app) fn waiting<'a>(
    items: &'a [Item],
    seen: &BTreeMap<String, Seen>,
) -> Vec<(&'a Item, Vec<Reason>)> {
    items
        .iter()
        .map(|i| {
            (
                i,
                forge_inbox::needs_you(&i.pr, seen.get(&i.pr.url).copied()),
            )
        })
        .filter(|(_, reasons)| !reasons.is_empty())
        .collect()
}

/// After a round: yours met for the first time are seen as they are, so that only what is
/// said from now on is news (not every comment they ever had); and what was seen of pull
/// requests no longer listed (merged, closed) is let go, where the forge that would list them
/// answered.
pub(super) fn remember_round(
    seen: &mut BTreeMap<String, Seen>,
    items: &[Item],
    covered: &[String],
) {
    for item in items.iter().filter(|i| i.pr.mine) {
        if !seen.contains_key(&item.pr.url) {
            seen.insert(item.pr.url.clone(), forge_inbox::seen_now(&item.pr, None));
        }
    }
    let listed: HashSet<&str> = items.iter().map(|i| i.pr.url.as_str()).collect();
    seen.retain(|url, _| {
        let url_lower = url.to_lowercase();
        listed.contains(url.as_str()) || !covered.iter().any(|c| url_lower.starts_with(c))
    });
}

/// The problems once each by forge, host and reason, with where each was met.
pub(super) fn grouped(problems: &[&Problem]) -> Vec<String> {
    let mut groups: Vec<(&Problem, Vec<&str>)> = Vec::new();
    for p in problems {
        match groups
            .iter_mut()
            .find(|(g, _)| g.forge == p.forge && g.host == p.host && g.why == p.why)
        {
            Some((_, at)) => {
                if !at.contains(&p.at.as_str()) {
                    at.push(&p.at);
                }
            }
            None => groups.push((p, vec![&p.at])),
        }
    }
    groups
        .into_iter()
        .map(|(p, at)| format!("{}: {}", at.join(", "), forge_inbox::display_error(&p.why)))
        .collect()
}
