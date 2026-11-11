//! Sign and group edits into recurring transformations.
use super::pattern::{Part, Signature, Token};
use super::*;
use std::collections::HashMap;

/// An edit's tokens and signature, and how it is grouped.
struct Signed {
    shape: Shape,
    old: Vec<Token>,
    new: Vec<Token>,
    sig: Signature,
    key: String,
}

/// A group of edits sharing a key, while they are grouped.
struct Group {
    shape: Shape,
    edits: Vec<usize>,
    before: Vec<Part>,
    after: Vec<Part>,
    /// The first edit's signature.
    sig: Signature,
    /// Its tokens.
    old: Vec<Token>,
    new: Vec<Token>,
    label: String,
}

/// Groups the edits of what was read.
pub fn analyze(subject: Subject, read: diff::Read, limits: &Limits) -> Transform {
    let mut files: Vec<String> = read.hunks.iter().map(|h| h.path.clone()).collect();
    files.sort();
    files.dedup();
    let file_of = |path: &str| {
        files
            .binary_search_by(|f| f.as_str().cmp(path))
            .unwrap_or(0)
    };
    let mut edits = Vec::new();
    for (index, h) in read.hunks.iter().enumerate() {
        let file = file_of(&h.path);
        if h.removed.len() == h.added.len() && h.removed.len() > 1 {
            for (i, (old, new)) in h.removed.iter().zip(&h.added).enumerate() {
                edits.push(Edit {
                    file,
                    hunk: index,
                    old: vec![old.clone()],
                    new: vec![new.clone()],
                    old_line: h.old_start + i as u32,
                    new_line: h.new_start + i as u32,
                    new_len: 1,
                });
            }
        } else {
            edits.push(Edit {
                file,
                hunk: index,
                old: h.removed.clone(),
                new: h.added.clone(),
                old_line: h.old_start,
                new_line: h.new_start,
                new_len: h.new_len,
            });
        }
    }
    let edits_cut = edits.len() > limits.max_edits;
    if edits_cut {
        // Whole hunks are kept: an edit of a hunk isn't shown without the rest of it.
        let last = edits[limits.max_edits].hunk;
        edits.retain(|e| e.hunk < last);
    }
    let signed: Vec<Signed> = edits
        .iter()
        .enumerate()
        .map(|(i, e)| sign(i, e, limits))
        .collect();
    let groups = group(&edits, &signed);
    let parents = deviations(&groups);
    let patterns = order(&groups, &parents, &edits);
    let mut provenance = Provenance::new("git diff -U0", subject.scope.clone());
    for (name, sha) in &subject.tips {
        provenance = provenance.tip(name.clone(), sha.clone());
    }
    let mut partial = Vec::new();
    if read.beyond > 0 {
        partial.push(format!(
            "only {} of the files changed are read ({} more aren't)",
            limits.max_files, read.beyond
        ));
    }
    if !read.too_large.is_empty() {
        partial.push(format!(
            "{} files with over {} lines changed aren't read",
            read.too_large.len(),
            limits.max_file_lines
        ));
    }
    if edits_cut {
        partial.push(format!(
            "only the first {} edits are grouped",
            limits.max_edits
        ));
    }
    if read.hunks_cut {
        partial.push(format!(
            "only the first {} hunks are read",
            limits.max_hunks
        ));
    }
    if !partial.is_empty() {
        provenance = provenance.partial(partial.join("; "));
    }
    Transform {
        subject,
        files,
        hunks: read.hunks,
        edits,
        patterns,
        provenance,
        binary: read.binary,
    }
}

/// An edit's tokens, shape and key.
fn sign(index: usize, edit: &Edit, limits: &Limits) -> Signed {
    let mut old = Vec::new();
    for line in &edit.old {
        pattern::tokenize(line, &mut old);
    }
    let mut new = Vec::new();
    for line in &edit.new {
        pattern::tokenize(line, &mut new);
    }
    if old.len() > limits.max_tokens || new.len() > limits.max_tokens {
        return Signed {
            shape: Shape::Large,
            old: Vec::new(),
            new: Vec::new(),
            sig: Signature {
                old: Vec::new(),
                new: Vec::new(),
                bindings: Vec::new(),
            },
            key: format!("\u{4}large{index}"),
        };
    }
    let sig = pattern::signature(&old, &new);
    let (shape, key) = if old == new {
        (Shape::Layout, "\u{4}layout".to_owned())
    } else if old.is_empty() {
        (Shape::Added, sig.key())
    } else if new.is_empty() {
        (Shape::Removed, sig.key())
    } else {
        (Shape::Rewrite, sig.key())
    };
    Signed {
        shape,
        old,
        new,
        sig,
        key,
    }
}

/// The edits grouped by key, each group with its template, the largest first.
fn group(edits: &[Edit], signed: &[Signed]) -> Vec<Group> {
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut members: Vec<Vec<usize>> = Vec::new();
    for (i, s) in signed.iter().enumerate() {
        let g = *index.entry(s.key.as_str()).or_insert_with(|| {
            members.push(Vec::new());
            members.len() - 1
        });
        members[g].push(i);
    }
    let mut groups: Vec<Group> = members
        .into_iter()
        .map(|list| {
            let first = &signed[list[0]];
            let bindings: Vec<&[String]> = list
                .iter()
                .map(|&e| signed[e].sig.bindings.as_slice())
                .collect();
            let (before, after) = pattern::template(&first.sig, &bindings);
            let label = match first.shape {
                Shape::Large | Shape::Layout => {
                    let e = &edits[list[0]];
                    format!("{} → {} lines", e.old.len(), e.new.len())
                }
                _ => format!(
                    "{} → {}",
                    pattern::render_like(&before, &first.old),
                    pattern::render_like(&after, &first.new)
                ),
            };
            Group {
                shape: first.shape,
                edits: list,
                before,
                after,
                sig: first.sig.clone(),
                old: first.old.clone(),
                new: first.new.clone(),
                label,
            }
        })
        .collect();
    groups.sort_by(|a, b| {
        b.edits
            .len()
            .cmp(&a.edits.len())
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.edits[0].cmp(&b.edits[0]))
    });
    groups
}

/// Which group each group deviates from (an index into `groups`), if any: the largest
/// repeated pattern whose removed side it fits while adding something else, or whose
/// substitution it mostly makes.
fn deviations(groups: &[Group]) -> Vec<Option<usize>> {
    let mut parent: Vec<Option<usize>> = vec![None; groups.len()];
    for g in 0..groups.len() {
        let me = &groups[g];
        if matches!(me.shape, Shape::Layout | Shape::Large) {
            continue;
        }
        let (my_old, my_new) = pattern::core(&me.sig);
        let found = (0..g).find(|&p| {
            let other = &groups[p];
            if parent[p].is_some()
                || other.edits.len() < 2
                || other.edits.len() <= me.edits.len()
                || matches!(other.shape, Shape::Layout | Shape::Large)
            {
                return false;
            }
            let fits = me.shape == Shape::Rewrite
                && other.shape == Shape::Rewrite
                && pattern::matches(&other.before, &me.old);
            if fits {
                return true;
            }
            let (old, new) = pattern::core(&other.sig);
            me.shape == other.shape
                && pattern::dice(&me.sig.old, &other.sig.old) >= 0.6
                && pattern::dice(&me.sig.new, &other.sig.new) >= 0.6
                && pattern::dice(
                    &[my_old.clone(), my_new.clone()].concat(),
                    &[old, new].concat(),
                ) >= 0.6
        });
        parent[g] = found;
    }
    parent
}

/// The patterns in the order shown: each repeated pattern followed by its deviations.
fn order(groups: &[Group], parents: &[Option<usize>], edits: &[Edit]) -> Vec<Pattern> {
    let mut sequence = Vec::with_capacity(groups.len());
    for g in 0..groups.len() {
        if parents[g].is_some() {
            continue;
        }
        sequence.push(g);
        sequence.extend((0..groups.len()).filter(|&d| parents[d] == Some(g)));
    }
    let mut at = vec![0; groups.len()];
    for (i, &g) in sequence.iter().enumerate() {
        at[g] = i;
    }
    let by_file = |list: &mut dyn Iterator<Item = usize>| -> Vec<(usize, u32)> {
        let mut counts: Vec<(usize, u32)> = Vec::new();
        let mut files: Vec<usize> = list.map(|e| edits[e].file).collect();
        files.sort_unstable();
        for f in files {
            match counts.last_mut() {
                Some((last, n)) if *last == f => *n += 1,
                _ => counts.push((f, 1)),
            }
        }
        counts
    };
    sequence
        .iter()
        .map(|&g| {
            let group = &groups[g];
            let deviants: Vec<usize> = (0..groups.len())
                .filter(|&d| parents[d] == Some(g))
                .collect();
            let (before, after) = match group.shape {
                Shape::Large | Shape::Layout => {
                    let e = &edits[group.edits[0]];
                    (
                        format!("{} lines", e.old.len()),
                        format!("{} lines", e.new.len()),
                    )
                }
                _ => (
                    pattern::render_like(&group.before, &group.old),
                    pattern::render_like(&group.after, &group.new),
                ),
            };
            Pattern {
                shape: group.shape,
                before,
                after,
                edits: group.edits.clone(),
                deviates_from: parents[g].map(|p| at[p]),
                deviating: deviants.iter().map(|&d| groups[d].edits.len()).sum(),
                per_file: by_file(&mut group.edits.iter().copied()),
                deviant_files: by_file(
                    &mut deviants
                        .iter()
                        .flat_map(|&d| groups[d].edits.iter().copied()),
                ),
            }
        })
        .collect()
}
