//! The reviewer's own arrangement over the rules' proposal: groups renamed, reordered, split
//! and merged, units moved. It is saved with the two revisions it was made at; at others (a
//! force push, new commits) it is stale, the proposal is shown, and what still matches can be
//! carried over on request.

use serde::{Deserialize, Serialize};

use super::propose::{Group, Proposal};
use super::units::Units;

/// A group as arranged.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::repo::insights) struct SavedGroup {
    pub(in crate::repo::insights) id: String,
    pub(in crate::repo::insights) title: String,
    #[serde(default)]
    pub(in crate::repo::insights) renamed: bool,
    /// The units placed in it by hand, by key.
    #[serde(default)]
    pub(in crate::repo::insights) units: Vec<String>,
}

/// An arrangement, made at `base`..`head`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::repo::insights) struct Saved {
    pub(in crate::repo::insights) base: String,
    pub(in crate::repo::insights) head: String,
    /// Every group, in the order shown.
    pub(in crate::repo::insights) groups: Vec<SavedGroup>,
    /// The number of the next group made by hand.
    #[serde(default)]
    pub(in crate::repo::insights) next: u64,
}

/// How a unit got where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo::insights) enum Placed {
    /// The rules put it there, for this reason.
    Rule(String),
    /// The reviewer did.
    Yours,
}

/// A group as shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo::insights) struct Shown {
    pub(in crate::repo::insights) group: Group,
    pub(in crate::repo::insights) renamed: bool,
    /// Made by hand, not by a rule.
    pub(in crate::repo::insights) yours: bool,
}

/// The groups shown, and how each unit got into its own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::repo::insights) struct Arranged {
    pub(in crate::repo::insights) groups: Vec<Shown>,
    pub(in crate::repo::insights) placed: Vec<Placed>,
    /// The revisions of an arrangement made before the pull request changed: not applied.
    pub(in crate::repo::insights) stale: Option<(String, String)>,
    /// An arrangement of the reviewer's is applied.
    pub(in crate::repo::insights) edited: bool,
    pub(in crate::repo::insights) next: u64,
}

/// The proposal with `saved` applied, if it was made at the revisions of `units`.
pub(in crate::repo::insights) fn arrange(
    proposal: &Proposal,
    units: &Units,
    saved: Option<&Saved>,
) -> Arranged {
    let mut placed: Vec<Placed> = proposal.reasons.iter().cloned().map(Placed::Rule).collect();
    let current = saved.filter(|s| s.base == units.base && s.head == units.head);
    let Some(saved) = current else {
        return Arranged {
            groups: proposal
                .groups
                .iter()
                .map(|g| Shown {
                    group: g.clone(),
                    renamed: false,
                    yours: false,
                })
                .collect(),
            placed,
            stale: saved.map(|s| (s.base.clone(), s.head.clone())),
            edited: false,
            next: 0,
        };
    };
    let mut shown: Vec<Shown> = Vec::new();
    for sg in &saved.groups {
        let mut s = match proposal.groups.iter().find(|g| g.id == sg.id) {
            Some(g) => Shown {
                group: g.clone(),
                renamed: false,
                yours: false,
            },
            None => Shown {
                group: Group {
                    id: sg.id.clone(),
                    title: sg.title.clone(),
                    why: "made by you".to_owned(),
                    purpose: None,
                    units: Vec::new(),
                },
                renamed: false,
                yours: true,
            },
        };
        if sg.renamed {
            s.group.title = sg.title.clone();
            s.renamed = true;
        }
        shown.push(s);
    }
    for g in &proposal.groups {
        if !shown.iter().any(|s| s.group.id == g.id) {
            shown.push(Shown {
                group: g.clone(),
                renamed: false,
                yours: false,
            });
        }
    }
    let at = |id: &str| shown.iter().position(|s| s.group.id == id);
    // Each unit in the group the rules chose, then where the reviewer put it.
    let mut group_of = vec![usize::MAX; units.units.len()];
    for g in &proposal.groups {
        let to = at(&g.id).unwrap_or(0);
        for &u in &g.units {
            group_of[u] = to;
        }
    }
    let by_key: std::collections::HashMap<&str, usize> = units
        .units
        .iter()
        .enumerate()
        .map(|(i, u)| (u.key.as_str(), i))
        .collect();
    for sg in &saved.groups {
        let Some(to) = at(&sg.id) else { continue };
        for key in &sg.units {
            if let Some(&u) = by_key.get(key.as_str()) {
                group_of[u] = to;
                placed[u] = Placed::Yours;
            }
        }
    }
    for s in &mut shown {
        s.group.units.clear();
    }
    for (u, &g) in group_of.iter().enumerate() {
        if let Some(s) = shown.get_mut(g) {
            s.group.units.push(u);
        }
    }
    shown.retain(|s| !s.group.units.is_empty());
    Arranged {
        groups: shown,
        placed,
        stale: None,
        edited: true,
        next: saved.next,
    }
}

/// Whether every unit of `units` is in exactly one group.
pub(in crate::repo::insights) fn check(arranged: &Arranged, units: &Units) -> Result<(), String> {
    let mut count = vec![0usize; units.units.len()];
    for g in &arranged.groups {
        for &u in &g.group.units {
            *count
                .get_mut(u)
                .ok_or("a group holds a change that isn't there")? += 1;
        }
    }
    match count.iter().position(|&n| n != 1) {
        Some(u) => Err(format!("{} is in {} groups", units.units[u].path, count[u])),
        None => Ok(()),
    }
}

/// A change made by hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    Rename {
        group: String,
        title: String,
    },
    /// Units (by key) to another group.
    Move {
        units: Vec<String>,
        to: String,
    },
    /// Units (by key) of `from` to a new group shown after it.
    Split {
        units: Vec<String>,
        from: String,
    },
    /// All of `from` into `into`.
    Merge {
        from: String,
        into: String,
    },
    /// The units of a group are where they belong.
    Confirm {
        group: String,
    },
    /// A group up or down the list.
    Shift {
        group: String,
        up: bool,
    },
}

/// The arrangement `edit` makes of what is shown.
pub(in crate::repo::insights) fn edit(
    arranged: &Arranged,
    units: &Units,
    edit: Edit,
) -> Result<Saved, String> {
    let mut saved = Saved {
        base: units.base.clone(),
        head: units.head.clone(),
        groups: arranged
            .groups
            .iter()
            .map(|s| SavedGroup {
                id: s.group.id.clone(),
                title: s.group.title.clone(),
                renamed: s.renamed,
                units: s
                    .group
                    .units
                    .iter()
                    .filter(|&&u| arranged.placed[u] == Placed::Yours)
                    .map(|&u| units.units[u].key.clone())
                    .collect(),
            })
            .collect(),
        next: arranged.next,
    };
    let find = |saved: &Saved, id: &str| {
        saved
            .groups
            .iter()
            .position(|g| g.id == id)
            .ok_or_else(|| "That group is gone: the grouping changed since".to_owned())
    };
    let keys_of = |id: &str| -> Vec<String> {
        arranged
            .groups
            .iter()
            .find(|s| s.group.id == id)
            .map(|s| {
                s.group
                    .units
                    .iter()
                    .map(|&u| units.units[u].key.clone())
                    .collect()
            })
            .unwrap_or_default()
    };
    match edit {
        Edit::Rename { group, title } => {
            let g = find(&saved, &group)?;
            let title = title.trim();
            if title.is_empty() {
                return Err("A group needs a name".to_owned());
            }
            saved.groups[g].title = title.to_owned();
            saved.groups[g].renamed = true;
        }
        Edit::Move { units: keys, to } => {
            let g = find(&saved, &to)?;
            place(&mut saved, &keys, g, units)?;
        }
        Edit::Split { units: keys, from } => {
            let f = find(&saved, &from)?;
            saved.next += 1;
            let title = format!("{} (split {})", saved.groups[f].title, saved.next);
            saved.groups.insert(
                f + 1,
                SavedGroup {
                    id: format!("user:{}", saved.next),
                    title,
                    renamed: true,
                    units: Vec::new(),
                },
            );
            place(&mut saved, &keys, f + 1, units)?;
        }
        Edit::Merge { from, into } => {
            let keys = keys_of(&from);
            let g = find(&saved, &into)?;
            find(&saved, &from)?;
            place(&mut saved, &keys, g, units)?;
            saved.groups.retain(|sg| sg.id != from);
        }
        Edit::Confirm { group } => {
            let keys = keys_of(&group);
            let g = find(&saved, &group)?;
            place(&mut saved, &keys, g, units)?;
        }
        Edit::Shift { group, up } => {
            let g = find(&saved, &group)?;
            let other = if up { g.checked_sub(1) } else { Some(g + 1) };
            if let Some(o) = other.filter(|&o| o < saved.groups.len()) {
                saved.groups.swap(g, o);
            }
        }
    }
    Ok(saved)
}

/// Puts the units `keys` into group `g` by hand, out of wherever else they were.
fn place(saved: &mut Saved, keys: &[String], g: usize, units: &Units) -> Result<(), String> {
    if keys.is_empty() {
        return Err("Nothing is selected".to_owned());
    }
    if let Some(k) = keys
        .iter()
        .find(|k| !units.units.iter().any(|u| &u.key == *k))
    {
        return Err(format!(
            "A change is gone ({k}): the pull request changed since"
        ));
    }
    for sg in &mut saved.groups {
        sg.units.retain(|k| !keys.contains(k));
    }
    saved.groups[g].units.extend(keys.iter().cloned());
    Ok(())
}

/// `saved` made at other revisions, brought to those of `units`: the units still there (by
/// content) stay where they were put; the others go where the rules put them. Also how many
/// placements were dropped.
pub(in crate::repo::insights) fn carry_over(saved: &Saved, units: &Units) -> (Saved, usize) {
    let mut dropped = 0;
    let groups = saved
        .groups
        .iter()
        .map(|sg| {
            let kept: Vec<String> = sg
                .units
                .iter()
                .filter(|k| units.units.iter().any(|u| &u.key == *k))
                .cloned()
                .collect();
            dropped += sg.units.len() - kept.len();
            SavedGroup {
                units: kept,
                ..sg.clone()
            }
        })
        .collect();
    (
        Saved {
            base: units.base.clone(),
            head: units.head.clone(),
            groups,
            next: saved.next,
        },
        dropped,
    )
}
