//! Classify paired patches and infer folded or split patches.
use super::*;

/// What the two versions' pairs are, with the patches read of their commits (by hash): the
/// commits classed and joined.
pub fn classify(diff: &RangeDiff, patches: &HashMap<String, Patch>) -> Ribbons {
    let mut old: Vec<(usize, &SeriesCommit)> = Vec::new();
    let mut new: Vec<(usize, &SeriesCommit)> = Vec::new();
    for (i, pair) in diff.pairs.iter().enumerate() {
        if let Some(c) = &pair.old {
            old.push((i, c));
        }
        if let Some(c) = &pair.new {
            new.push((i, c));
        }
    }
    old.sort_by_key(|(_, c)| c.index);
    new.sort_by_key(|(_, c)| c.index);
    let place =
        |list: &[(usize, &SeriesCommit)], pair: usize| list.iter().position(|(p, _)| *p == pair);
    let mut out = Ribbons {
        old: old
            .iter()
            .map(|(_, c)| End {
                commit: (*c).clone(),
                class: Class::Dropped,
                moved: false,
            })
            .collect(),
        new: new
            .iter()
            .map(|(_, c)| End {
                commit: (*c).clone(),
                class: Class::Added,
                moved: false,
            })
            .collect(),
        diff: diff.clone(),
        ..Ribbons::default()
    };
    for (i, pair) in diff.pairs.iter().enumerate() {
        let (Some(o), Some(n)) = (place(&old, i), place(&new, i)) else {
            continue;
        };
        let (class, evidence, why) = paired(pair, patches);
        out.ribbons.push(Ribbon {
            old: o,
            new: n,
            class,
            evidence,
            moved: pair.moved,
            pair: Some(i),
            why,
        });
    }
    infer_folds(&mut out, patches);
    settle_classes(&mut out);
    out
}

/// What a pair range-diff made is: by its mark, and by whether the two patch-ids are the same.
fn paired(pair: &Pair, patches: &HashMap<String, Patch>) -> (Class, Evidence, String) {
    let id = |c: &Option<SeriesCommit>| {
        c.as_ref()
            .and_then(|c| patches.get(&c.hash))
            .and_then(|p| p.id.clone())
    };
    let (a, b) = (id(&pair.old), id(&pair.new));
    match pair.kind {
        PairKind::Same if pair.moved => (
            Class::Reordered,
            Evidence::Observed,
            "range-diff: the same patch and message, moved".to_owned(),
        ),
        PairKind::Same => (
            Class::Unchanged,
            Evidence::Observed,
            "range-diff: the same patch and message".to_owned(),
        ),
        _ => match (a, b) {
            (Some(a), Some(b)) if a == b && message_changed(&pair.body) => (
                Class::Reworded,
                Evidence::Observed,
                "the same patch-id; range-diff: the message changed".to_owned(),
            ),
            // patch-id doesn't see whitespace: the patch changed in that alone.
            (Some(a), Some(b)) if a == b => (
                Class::Modified,
                Evidence::Observed,
                "the same patch-id and message; range-diff: the patch changed in whitespace"
                    .to_owned(),
            ),
            (Some(_), Some(_)) => (
                Class::Modified,
                Evidence::Inferred,
                "range-diff paired them by similarity; their patch-ids differ".to_owned(),
            ),
            _ => (
                Class::Modified,
                Evidence::Inferred,
                "range-diff paired them by similarity; patch-ids not read".to_owned(),
            ),
        },
    }
}

/// Whether range-diff's `body` of a pair changes a line of its commit message section. A
/// section starts at range-diff's own hunk header (`@@ Commit message`), or at the patch's
/// section line (` ## Commit message ##`) shown as context; a message's own lines are indented,
/// so a `## Summary` in it isn't taken for one.
pub(super) fn message_changed(body: &[String]) -> bool {
    let mut in_message = false;
    for line in body {
        if let Some(section) = line.strip_prefix("@@") {
            in_message = section.trim() == "Commit message";
            continue;
        }
        let mut chars = line.chars();
        let mark = chars.next();
        let rest = chars.as_str();
        if rest.starts_with(" ## ") && rest.trim_end().ends_with(" ##") {
            in_message = rest.trim() == "## Commit message ##";
        } else if in_message && matches!(mark, Some('-' | '+')) {
            return true;
        }
    }
    false
}

/// How many of `lines` are found in `other`, each line of `other` counted once.
fn found_in(lines: &[String], other: &HashMap<&str, usize>) -> usize {
    let mut left = other.clone();
    lines
        .iter()
        .filter(|l| match left.get_mut(l.as_str()) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        })
        .count()
}

fn bag<'a>(lists: impl IntoIterator<Item = &'a [String]>) -> HashMap<&'a str, usize> {
    let mut bag: HashMap<&str, usize> = HashMap::new();
    for list in lists {
        for line in list {
            *bag.entry(line.as_str()).or_default() += 1;
        }
    }
    bag
}

/// The commits range-diff left unpaired, joined to those of the other version holding most of
/// their changed lines: an old one folded into a new one paired already is squashed, a new one
/// made of an old one paired already was split off it, one found spread over several of the
/// other side is split (or squashed) across them. Every such ribbon is inferred.
fn infer_folds(out: &mut Ribbons, patches: &HashMap<String, Patch>) {
    let lines_of = |c: &SeriesCommit| -> &[String] {
        patches
            .get(&c.hash)
            .map(|p| p.lines.as_slice())
            .unwrap_or(&[])
    };
    let linked_new = |out: &Ribbons, n: usize| out.ribbons.iter().any(|r| r.new == n);
    let linked_old = |out: &Ribbons, o: usize| out.ribbons.iter().any(|r| r.old == o);
    // The old commits unpaired, against every new one.
    for o in 0..out.old.len() {
        if linked_old(out, o) {
            continue;
        }
        let lines = lines_of(&out.old[o].commit);
        if lines.is_empty() {
            continue;
        }
        let shares: Vec<(usize, f32)> = (0..out.new.len())
            .map(|n| {
                let other = bag([lines_of(&out.new[n].commit)]);
                (n, found_in(lines, &other) as f32 / lines.len() as f32)
            })
            .filter(|(_, s)| *s >= PART)
            .collect();
        let Some(&(best, share)) = shares.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else {
            continue;
        };
        if share >= FOLDED {
            let (class, why) = if linked_new(out, best) {
                (
                    Class::Squashed,
                    format!(
                        "{:.0}% of its changed lines are in this commit, which holds another \
                         old commit too",
                        share * 100.0
                    ),
                )
            } else {
                (
                    Class::Modified,
                    format!(
                        "range-diff didn't pair them; {:.0}% of its changed lines are in this \
                         commit",
                        share * 100.0
                    ),
                )
            };
            push_inferred(out, o, best, class, why);
            continue;
        }
        let together = bag(shares.iter().map(|(n, _)| lines_of(&out.new[*n].commit)));
        let union = found_in(lines, &together) as f32 / lines.len() as f32;
        if shares.len() >= 2 && union >= FOLDED {
            for (n, share) in &shares {
                let why = format!(
                    "{:.0}% of its changed lines are in this commit, {:.0}% across {} commits",
                    share * 100.0,
                    union * 100.0,
                    shares.len()
                );
                push_inferred(out, o, *n, Class::Split, why);
            }
        }
    }
    // The new commits still unpaired, against every old one.
    for n in 0..out.new.len() {
        if linked_new(out, n) {
            continue;
        }
        let lines = lines_of(&out.new[n].commit);
        if lines.is_empty() {
            continue;
        }
        let shares: Vec<(usize, f32)> = (0..out.old.len())
            .map(|o| {
                let other = bag([lines_of(&out.old[o].commit)]);
                (o, found_in(lines, &other) as f32 / lines.len() as f32)
            })
            .filter(|(_, s)| *s >= PART)
            .collect();
        let Some(&(best, share)) = shares.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else {
            continue;
        };
        if share >= FOLDED {
            let class = if linked_old(out, best) {
                Class::Split
            } else {
                Class::Modified
            };
            let why = format!(
                "{:.0}% of this commit's changed lines come from the old one",
                share * 100.0
            );
            push_inferred(out, best, n, class, why);
            continue;
        }
        let together = bag(shares.iter().map(|(o, _)| lines_of(&out.old[*o].commit)));
        let union = found_in(lines, &together) as f32 / lines.len() as f32;
        if shares.len() >= 2 && union >= FOLDED {
            for (o, share) in &shares {
                let why = format!(
                    "{:.0}% of this commit's changed lines come from the old one, {:.0}% from \
                     {} old commits",
                    share * 100.0,
                    union * 100.0,
                    shares.len()
                );
                push_inferred(out, *o, n, Class::Squashed, why);
            }
        }
    }
}

fn push_inferred(out: &mut Ribbons, old: usize, new: usize, class: Class, why: String) {
    out.ribbons.push(Ribbon {
        old,
        new,
        class,
        evidence: Evidence::Inferred,
        moved: false,
        pair: None,
        why,
    });
}

/// A new commit several old ones lead into is a squash, an old one leading to several new ones
/// a split, whatever range-diff called its pair; each commit then takes the strongest class of
/// its ribbons.
fn settle_classes(out: &mut Ribbons) {
    let into = |out: &Ribbons, n: usize| out.ribbons.iter().filter(|r| r.new == n).count();
    let from = |out: &Ribbons, o: usize| out.ribbons.iter().filter(|r| r.old == o).count();
    let classes: Vec<Class> = out
        .ribbons
        .iter()
        .map(|r| {
            if into(out, r.new) >= 2 {
                Class::Squashed
            } else if from(out, r.old) >= 2 {
                Class::Split
            } else {
                r.class
            }
        })
        .collect();
    for (r, class) in out.ribbons.iter_mut().zip(classes) {
        r.class = class;
    }
    for r in &out.ribbons {
        for end in [&mut out.old[r.old], &mut out.new[r.new]] {
            if matches!(end.class, Class::Dropped | Class::Added)
                || r.class.weight() > end.class.weight()
            {
                end.class = r.class;
            }
            end.moved |= r.moved;
        }
    }
}
