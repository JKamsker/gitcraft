//! `RepoTab` field inventory: readers and writers per field and domain, cross-feature
//! mutations, and hotspots. A large field count alone is never a finding.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::domains::SAMPLES;
use crate::model::{Inventory, Ns};
use crate::policy::Policy;
use crate::refs::Access;
use crate::resolve::Resolver;
use crate::tab::{Mode, TabInfo};

#[derive(Clone, Debug, Serialize)]
pub struct Field {
    pub name: String,
    pub owner: String,
    pub visibility: String,
    pub line: usize,
    pub readers: BTreeMap<String, usize>,
    pub mutators: BTreeMap<String, BTreeMap<Mode, usize>>,
    pub init: usize,
    pub test_accesses: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Site {
    pub file: String,
    pub line: usize,
    pub mode: Mode,
}

#[derive(Clone, Debug, Serialize)]
pub struct CrossMutation {
    pub domain: String,
    pub field: String,
    pub owner: String,
    pub count: usize,
    pub samples: Vec<Site>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Hotspot {
    pub name: String,
    pub domains_reading: usize,
    pub domains_mutating: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DomainUse {
    pub domain: String,
    pub fields_read: usize,
    pub fields_mutated: usize,
    pub foreign_fields_mutated: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct TabReport {
    pub path: String,
    pub found: bool,
    pub file: String,
    pub fields: Vec<Field>,
    pub cross_mutations: Vec<CrossMutation>,
    pub field_hotspots: Vec<Hotspot>,
    pub domain_use: Vec<DomainUse>,
}

/// Each field's owning domain: explicit in the policy, else the feature that defines its type.
pub fn owners(
    res: &Resolver,
    tab: &TabInfo,
    domains: &[String],
    policy: &Policy,
) -> BTreeMap<String, String> {
    let rules = &policy.repo_tab;
    tab.fields
        .iter()
        .map(|(name, (_, _, ty))| {
            let derived = tab.module.and_then(|module| {
                res.resolve(module, ty, Ns::Type, false, &[])
                    .into_iter()
                    .filter_map(|def| crate::domains::def_module(res.inv, def))
                    .map(|m| &domains[m])
                    .find(|d| {
                        policy
                            .domains
                            .get(*d)
                            .is_some_and(|s| rules.owner_layers.contains(&s.layer))
                    })
            });
            let owner = rules
                .explicit_owner(name)
                .or(derived.map(String::as_str))
                .unwrap_or(&rules.default_owner);
            (name.clone(), owner.to_owned())
        })
        .collect()
}

pub fn report(
    inv: &Inventory,
    tab: &TabInfo,
    accesses: &[Access],
    domains: &[String],
    policy: &Policy,
    owners: &BTreeMap<String, String>,
) -> TabReport {
    let rules = &policy.repo_tab;
    let mut fields: BTreeMap<String, Field> = tab
        .fields
        .iter()
        .map(|(name, (vis, line, _))| {
            let field = Field {
                name: name.clone(),
                owner: owners.get(name).cloned().unwrap_or_default(),
                visibility: vis.clone(),
                line: *line,
                readers: BTreeMap::new(),
                mutators: BTreeMap::new(),
                init: 0,
                test_accesses: 0,
            };
            (name.clone(), field)
        })
        .collect();
    let mut cross: BTreeMap<(String, String), (usize, BTreeSet<Site>)> = BTreeMap::new();
    for access in accesses {
        let Some(field) = fields.get_mut(&access.field) else {
            continue;
        };
        if access.site.test || inv.crates[inv.modules[access.module].krate].test {
            field.test_accesses += 1;
            continue;
        }
        let domain = &domains[access.module];
        match access.mode {
            Mode::Init => field.init += 1,
            Mode::Read => *field.readers.entry(domain.clone()).or_default() += 1,
            mode => {
                *field
                    .mutators
                    .entry(domain.clone())
                    .or_default()
                    .entry(mode)
                    .or_default() += 1
            }
        }
        if access.mode.is_mutation()
            && *domain != field.owner
            && !rules.coordinators.contains(domain)
        {
            let entry = cross
                .entry((domain.clone(), access.field.clone()))
                .or_default();
            entry.0 += 1;
            entry.1.insert(Site {
                file: access.site.file.clone(),
                line: access.site.line,
                mode: access.mode,
            });
            if entry.1.len() > SAMPLES {
                entry.1.pop_last();
            }
        }
    }
    let cross_mutations = cross
        .into_iter()
        .map(|((domain, field), (count, samples))| CrossMutation {
            owner: owners.get(&field).cloned().unwrap_or_default(),
            domain,
            field,
            count,
            samples: samples.into_iter().collect(),
        })
        .collect();
    let mut field_hotspots: Vec<Hotspot> = fields
        .values()
        .map(|f| Hotspot {
            name: f.name.clone(),
            domains_reading: f.readers.len(),
            domains_mutating: f.mutators.len(),
        })
        .collect();
    field_hotspots.sort_by(|a, b| {
        (b.domains_mutating, b.domains_reading, &a.name).cmp(&(
            a.domains_mutating,
            a.domains_reading,
            &b.name,
        ))
    });
    let domain_use = domain_use(fields.values());
    let file = tab
        .item
        .map(|i| inv.items[i].site.file.clone())
        .unwrap_or_default();
    TabReport {
        path: rules.path.clone(),
        found: tab.item.is_some(),
        file,
        fields: fields.into_values().collect(),
        cross_mutations,
        field_hotspots,
        domain_use,
    }
}

fn domain_use<'a>(fields: impl Iterator<Item = &'a Field>) -> Vec<DomainUse> {
    let mut map: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    for f in fields {
        for domain in f.readers.keys() {
            map.entry(domain.clone()).or_default().0 += 1;
        }
        for domain in f.mutators.keys() {
            let entry = map.entry(domain.clone()).or_default();
            entry.1 += 1;
            entry.2 += usize::from(*domain != f.owner);
        }
    }
    let mut out: Vec<DomainUse> = map
        .into_iter()
        .map(
            |(domain, (fields_read, fields_mutated, foreign_fields_mutated))| DomainUse {
                domain,
                fields_read,
                fields_mutated,
                foreign_fields_mutated,
            },
        )
        .collect();
    out.sort_by(|a, b| {
        (b.foreign_fields_mutated, b.fields_mutated, &a.domain).cmp(&(
            a.foreign_fields_mutated,
            a.fields_mutated,
            &b.domain,
        ))
    });
    out
}
