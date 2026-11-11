//! Bound the association of history and stash tree layers with candidate blobs.
use super::*;

pub(super) fn collect(
    reader: &Reader<'_>,
    commits: &BTreeSet<String>,
    unreachable: &BTreeSet<String>,
    live: &BTreeSet<String>,
    blobs: &mut BTreeMap<String, Vec<Provenance>>,
    report: &mut Report,
) -> Result<(), String> {
    let all = read_commits(reader, commits, report)?;
    let parents: BTreeSet<_> = all
        .values()
        .filter(|commit| {
            matches!(commit.parents.len(), 2 | 3)
                && (commit.subject.starts_with("WIP on ") || commit.subject.starts_with("On "))
        })
        .flat_map(|commit| commit.parents.iter().skip(1).cloned())
        .collect();
    let extra = read_commits(reader, &parents, report)?;
    let mut trees = BTreeMap::<String, Vec<(String, String, String)>>::new();
    let mut entries = 0;
    let mut ordered: Vec<_> = all.values().collect();
    ordered.sort_by(|a, b| b.when.cmp(&a.when).then_with(|| a.oid.cmp(&b.oid)));
    for commit in ordered {
        let dropped = stash_shape(commit, &extra);
        let origin = if dropped {
            if live.contains(&commit.oid) {
                Origin::LiveStash
            } else {
                Origin::DroppedStash
            }
        } else if unreachable.contains(&commit.oid) {
            Origin::Unreachable
        } else {
            Origin::History
        };
        let mut layers = vec![(&commit.tree, "working")];
        if dropped {
            for (i, parent) in commit.parents.iter().enumerate().skip(1) {
                if let Some(parent) = extra.get(parent) {
                    layers.push((&parent.tree, if i == 1 { "staged" } else { "untracked" }));
                }
            }
        }
        for (tree, layer) in layers {
            if !reader.active() {
                return Err("Content recovery cancelled or timed out".into());
            }
            if !trees.contains_key(tree) {
                trees.insert(tree.clone(), tree_entries(reader, tree, report)?);
            }
            for (path, mode, blob) in &trees[tree] {
                if entries >= ENTRIES || (blobs.len() >= OBJECTS && !blobs.contains_key(blob)) {
                    report.limited = true;
                    break;
                }
                entries += 1;
                let sources = blobs.entry(blob.clone()).or_default();
                if sources.len() >= 50 {
                    report.limited = true;
                    continue;
                }
                sources.push(Provenance {
                    commit: commit.oid.clone(),
                    tree: tree.clone(),
                    path: path.clone(),
                    mode: mode.clone(),
                    when: commit.when,
                    subject: commit.subject.clone(),
                    origin: origin.clone(),
                    layer,
                });
            }
            if entries >= ENTRIES {
                break;
            }
        }
        if entries >= ENTRIES {
            break;
        }
    }
    Ok(())
}
