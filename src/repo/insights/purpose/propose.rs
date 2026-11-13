//! The groups the rules propose: every unit in exactly one, each group saying why it exists
//! and each unit why it is in it. Runtime code made of files of one name in several folders
//! (a feature across layers) is a group of its own.

use std::collections::{BTreeSet, HashMap};

use super::rules::{self, Purpose, Target};
use super::units::Units;

/// A group of units, proposed or arranged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo::insights) struct Group {
    /// Who it is across readings: the rule that makes it, or `user:n` for one made by hand.
    pub(in crate::repo::insights) id: String,
    pub(in crate::repo::insights) title: String,
    /// Why it exists, in words.
    pub(in crate::repo::insights) why: String,
    /// The purpose it was proposed for (`None` for one named by the repository or made by
    /// hand): which insight it links to.
    pub(in crate::repo::insights) purpose: Option<Purpose>,
    /// Indices into [`Units::units`], in their standard order.
    pub(in crate::repo::insights) units: Vec<usize>,
}

/// What the rules propose.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::repo::insights) struct Proposal {
    pub(in crate::repo::insights) groups: Vec<Group>,
    /// Why each unit is where the rules put it, by unit.
    pub(in crate::repo::insights) reasons: Vec<String>,
    /// Lines of the repository's rules file that weren't understood.
    pub(in crate::repo::insights) errors: Vec<String>,
}

/// File names that say less than their folder does.
const PLAIN_STEMS: [&str; 6] = ["mod", "index", "lib", "main", "__init__", "init"];

/// A file's feature name and its folder: its name without extensions, or its folder's name
/// for a `mod.rs`, an `index.ts`…
fn feature_of(path: &str) -> (String, String) {
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    let stem = name.split('.').next().unwrap_or(name);
    if PLAIN_STEMS.contains(&stem) {
        let (parent, folder) = dir.rsplit_once('/').unwrap_or(("", dir));
        return (folder.to_owned(), parent.to_owned());
    }
    (stem.to_owned(), dir.to_owned())
}

fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{cut}…")
}

/// The groups of `units`. `generated`: why the file folding rules fold a path, if they do.
pub(in crate::repo::insights) fn propose(
    units: &Units,
    generated: &dyn Fn(&str) -> Option<String>,
) -> Proposal {
    let ruled = &units.ruled;
    let own = &ruled.rules;
    let errors = ruled.errors.clone();
    let placements: Vec<rules::Placement> = units
        .units
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let hit = ruled.hits.get(i).copied().flatten();
            let hit = hit.map(|(r, p)| (&own[r], own[r].patterns[p].as_str()));
            rules::place(u, hit, generated(&u.path).as_deref())
        })
        .collect();
    let mut reasons: Vec<String> = placements.iter().map(|p| p.why.clone()).collect();
    // Runtime files sharing a name across folders.
    let mut folders: HashMap<String, BTreeSet<String>> = HashMap::new();
    for (u, p) in units.units.iter().zip(&placements) {
        if p.target == Target::Purpose(Purpose::Logic) {
            let (name, dir) = feature_of(&u.path);
            folders.entry(name).or_default().insert(dir);
        }
    }
    // (rank, order within it): what sorts the groups.
    let mut found: Vec<((usize, usize), Group)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (i, (unit, placement)) in units.units.iter().zip(&placements).enumerate() {
        let (rank, id, title, why, purpose) = match &placement.target {
            Target::Named(name) => (
                (
                    0,
                    own.iter()
                        .position(|r| r.target == placement.target)
                        .unwrap_or(0),
                ),
                format!("named:{name}"),
                name.clone(),
                format!("named by the rules in {}", super::units::CONFIG),
                None,
            ),
            Target::Purpose(Purpose::Logic) => {
                let (name, _) = feature_of(&unit.path);
                match folders.get(&name).filter(|dirs| dirs.len() > 1) {
                    Some(dirs) => {
                        let list: Vec<&str> = dirs
                            .iter()
                            .map(|d| if d.is_empty() { "the top folder" } else { d })
                            .collect();
                        reasons[i] = format!("named `{name}` like files in other folders");
                        (
                            (1 + Purpose::Logic as usize, 0),
                            format!("feature:{name}"),
                            format!("Feature: {name}"),
                            format!("runtime files named `{name}` in {}", list.join(", ")),
                            Some(Purpose::Logic),
                        )
                    }
                    None => plain(Purpose::Logic, 1),
                }
            }
            Target::Purpose(Purpose::Repeated) => {
                let r = unit.repeated.map_or(0, |(r, _)| r);
                let made = units.repeated.get(r);
                (
                    (1 + Purpose::Repeated as usize, r),
                    format!("repeated:{}", made.map_or("", |m| m.label.as_str())),
                    format!(
                        "Repeated edit: {}",
                        shorten(made.map_or("", |m| m.label.as_str()), 60)
                    ),
                    format!(
                        "the same edit made in {} files, found by Repeated edits",
                        made.map_or(0, |m| m.files)
                    ),
                    Some(Purpose::Repeated),
                )
            }
            Target::Purpose(p) => plain(*p, 0),
        };
        let g = *index.entry(id.clone()).or_insert_with(|| {
            found.push((
                rank,
                Group {
                    id,
                    title,
                    why,
                    purpose,
                    units: Vec::new(),
                },
            ));
            found.len() - 1
        });
        found[g].1.units.push(i);
    }
    found.sort_by(|(a, x), (b, y)| a.cmp(b).then(x.units[0].cmp(&y.units[0])));
    Proposal {
        groups: found.into_iter().map(|(_, g)| g).collect(),
        reasons,
        errors,
    }
}

/// A purpose's own group: (rank, id, title, why, purpose).
fn plain(p: Purpose, within: usize) -> ((usize, usize), String, String, String, Option<Purpose>) {
    (
        (1 + p as usize, within),
        format!("purpose:{p:?}"),
        p.title().to_owned(),
        format!(
            "proposed by the built-in rules for {}",
            p.title().to_lowercase()
        ),
        Some(p),
    )
}
