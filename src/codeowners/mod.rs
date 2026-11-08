//! CODEOWNERS: who owns which files of a repository, as GitHub, GitLab and Gitea / Forgejo read
//! the file. Each forge looks for it in its own places, and reads it its own way:
//!
//! - GitHub: `.github/CODEOWNERS`, `CODEOWNERS`, `docs/CODEOWNERS`. gitignore-like patterns; the
//!   last line that matches a file says its owners (a line without owners: nobody).
//! - GitLab: `CODEOWNERS`, `docs/CODEOWNERS`, `.gitlab/CODEOWNERS`. The same patterns, in
//!   sections (`[Docs]`, `^[Optional]`, `[Approvals][2] @default-owner`): in each, the last
//!   line that matches counts, and the owners of every section add up. `!pattern` takes files
//!   out of a section.
//! - Gitea / Forgejo: `CODEOWNERS`, `docs/CODEOWNERS`, `.gitea/CODEOWNERS` (and then
//!   `.forgejo/CODEOWNERS` on Forgejo). Each pattern is a regular expression matching the whole
//!   path (`^pattern$`, as their released versions compile it); every line that matches adds its
//!   owners, and `!regex` stands for the files it doesn't match.
//!
//! GitHub, GitLab and Gitea read the file of the pull request's base branch: the reviewers
//! suggested for one are read from there too.

mod glob;
mod parse;
#[cfg(test)]
mod tests;

use std::path::Path;

use crate::forge::Forge;
use crate::git;

pub(crate) use parse::Problem;

/// How the file is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flavor {
    GitHub,
    GitLab,
    Gitea,
}

impl Flavor {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Flavor::GitHub => "GitHub",
            Flavor::GitLab => "GitLab",
            Flavor::Gitea => "Gitea / Forgejo",
        }
    }
}

/// Where each forge looks for the file, in the order it looks; the first found is the one.
pub(crate) fn locations(forge: Option<Forge>) -> Vec<(&'static str, Flavor)> {
    match forge {
        Some(Forge::GitHub) => vec![
            (".github/CODEOWNERS", Flavor::GitHub),
            ("CODEOWNERS", Flavor::GitHub),
            ("docs/CODEOWNERS", Flavor::GitHub),
        ],
        Some(Forge::GitLab) => vec![
            ("CODEOWNERS", Flavor::GitLab),
            ("docs/CODEOWNERS", Flavor::GitLab),
            (".gitlab/CODEOWNERS", Flavor::GitLab),
        ],
        Some(Forge::Forgejo) => vec![
            ("CODEOWNERS", Flavor::Gitea),
            ("docs/CODEOWNERS", Flavor::Gitea),
            (".gitea/CODEOWNERS", Flavor::Gitea),
            (".forgejo/CODEOWNERS", Flavor::Gitea),
        ],
        Some(Forge::Gitea) => vec![
            ("CODEOWNERS", Flavor::Gitea),
            ("docs/CODEOWNERS", Flavor::Gitea),
            (".gitea/CODEOWNERS", Flavor::Gitea),
        ],
        // No forge known: GitHub's places, then each other forge's own folder, which says
        // whose file it is.
        Some(Forge::Azure) => Vec::new(),
        None => vec![
            (".github/CODEOWNERS", Flavor::GitHub),
            ("CODEOWNERS", Flavor::GitHub),
            ("docs/CODEOWNERS", Flavor::GitHub),
            (".gitlab/CODEOWNERS", Flavor::GitLab),
            (".gitea/CODEOWNERS", Flavor::Gitea),
            (".forgejo/CODEOWNERS", Flavor::Gitea),
        ],
    }
}

/// A CODEOWNERS file, read.
#[derive(Clone, Debug)]
pub(crate) struct CodeOwners {
    /// Where it is in the repository.
    pub path: String,
    pub flavor: Flavor,
    /// The blob it was read from: the same blob needs no reading again.
    pub blob: String,
    /// The forge it was read for: another one reads it otherwise.
    pub forge: Option<Forge>,
    rules: Vec<parse::Rule>,
    sections: Vec<parse::Section>,
    /// Rule positions per section, including repeated section headers, in file order.
    section_rules: Vec<Vec<usize>>,
    /// Lines the forge would skip or misread, and why.
    pub problems: Vec<Problem>,
}

/// Where a CODEOWNERS file is at `rev`, and its blob: the first of `forge`'s places that has one.
pub(crate) fn find(
    repo: &Path,
    rev: &str,
    forge: Option<Forge>,
) -> Result<Option<(String, Flavor, String)>, String> {
    let places = locations(forge);
    let mut args = vec!["ls-tree", "-z", rev, "--"];
    args.extend(places.iter().map(|(p, _)| *p));
    // A repository without commits has nothing at HEAD.
    let listed = match git::run(Some(repo), &args) {
        Ok(out) => out,
        Err(_) if git::resolve(repo, rev).is_err() => return Ok(None),
        Err(e) => return Err(e),
    };
    // `mode type blob\tpath`, each ended by a NUL.
    let found: Vec<(&str, &str)> = listed
        .split('\0')
        .filter_map(|entry| {
            let (meta, path) = entry.split_once('\t')?;
            let mut meta = meta.split_whitespace();
            let (_, kind, blob) = (meta.next()?, meta.next()?, meta.next()?);
            (kind == "blob").then_some((path, blob))
        })
        .collect();
    Ok(places.into_iter().find_map(|(place, flavor)| {
        found
            .iter()
            .find(|(path, _)| *path == place)
            .map(|(path, blob)| ((*path).to_owned(), flavor, (*blob).to_owned()))
    }))
}

/// Reads the CODEOWNERS file of the repository at `rev`, as `forge` would (`None` when it has
/// none). `known` is the file read before: when its blob is still the one, it is kept as is.
pub(crate) fn read(
    repo: &Path,
    rev: &str,
    forge: Option<Forge>,
    known: Option<&CodeOwners>,
) -> Result<Option<CodeOwners>, String> {
    let Some((path, flavor, blob)) = find(repo, rev, forge)? else {
        return Ok(None);
    };
    if let Some(known) = known.filter(|k| k.blob == blob && k.path == path && k.forge == forge) {
        return Ok(Some(known.clone()));
    }
    let bytes = git::run_bytes(Some(repo), &["cat-file", "blob", &blob])?;
    let text = String::from_utf8_lossy(&bytes);
    let mut owners = CodeOwners::parse(&path, flavor, &text, forge.is_none());
    owners.blob = blob;
    owners.forge = forge;
    Ok(Some(owners))
}

impl CodeOwners {
    /// Reads `text`, the file at `path`, the `flavor`'s way. When the forge isn't known
    /// (`guessed`), a file found where GitHub looks that has sections (`[Name]`) is read as
    /// GitLab's.
    pub(crate) fn parse(path: &str, flavor: Flavor, text: &str, guessed: bool) -> Self {
        let flavor = match flavor {
            Flavor::GitHub if guessed && parse::has_sections(text) => Flavor::GitLab,
            other => other,
        };
        let parsed = parse::parse(flavor, text);
        let mut section_rules = vec![Vec::new(); parsed.sections.len()];
        if flavor == Flavor::GitLab {
            for (i, rule) in parsed.rules.iter().enumerate() {
                section_rules[rule.section].push(i);
            }
        }
        CodeOwners {
            path: path.to_owned(),
            flavor,
            blob: String::new(),
            forge: None,
            rules: parsed.rules,
            sections: parsed.sections,
            section_rules,
            problems: parsed.problems,
        }
    }

    /// The owners of the file at `path`, each once, in the order the file names them.
    pub(crate) fn owners(&self, path: &str) -> Vec<String> {
        let mut owners: Vec<String> = Vec::new();
        let mut add = |list: &[String]| {
            for owner in list {
                if !owners.iter().any(|o| o.eq_ignore_ascii_case(owner)) {
                    owners.push(owner.clone());
                }
            }
        };
        match self.flavor {
            Flavor::GitHub => {
                if let Some(rule) = self.rules.iter().rev().find(|r| r.matches(path)) {
                    add(&rule.owners);
                }
            }
            Flavor::GitLab => {
                // In each section the last line that matches counts, unless an exclusion
                // anywhere in it matches: then the section has nothing to say of the file.
                for indices in &self.section_rules {
                    let in_section = || indices.iter().map(|&i| &self.rules[i]);
                    if in_section().any(|r| r.negated && r.matches(path)) {
                        continue;
                    }
                    if let Some(rule) = in_section().rev().find(|r| !r.negated && r.matches(path)) {
                        add(&rule.owners);
                    }
                }
            }
            Flavor::Gitea => {
                for rule in self.rules.iter().filter(|r| r.matches(path) != r.negated) {
                    add(&rule.owners);
                }
            }
        }
        owners
    }

    /// The rules of the file that match none of `files` (the repository's), and the files
    /// that no rule gives an owner.
    pub(crate) fn lint(&self, files: &[String]) -> Lint {
        // A Gitea `!regex` applies to the files it doesn't match: to none when it matches all.
        let applies = |rule: &parse::Rule, f: &str| {
            rule.matches(f) != (rule.negated && self.flavor == Flavor::Gitea)
        };
        let unmatched = self
            .rules
            .iter()
            .filter(|rule| !files.iter().any(|f| applies(rule, f)))
            .map(|rule| (rule.line, rule.pattern.clone()))
            .collect();
        let unowned = files
            .iter()
            .filter(|f| self.owners(f).is_empty())
            .cloned()
            .collect();
        Lint {
            unmatched,
            unowned,
            files: files.len(),
        }
    }

    /// GitLab's sections, as the check shows them: `Docs`, `Database (optional, 2 approvals)`.
    pub(crate) fn section_labels(&self) -> Vec<String> {
        self.sections
            .iter()
            .filter(|s| !s.name.is_empty())
            .map(|s| {
                let mut notes = Vec::new();
                if s.optional {
                    notes.push("optional".to_owned());
                }
                if s.approvals > 1 {
                    notes.push(format!("{} approvals", s.approvals));
                }
                if notes.is_empty() {
                    s.name.clone()
                } else {
                    format!("{} ({})", s.name, notes.join(", "))
                }
            })
            .collect()
    }

    /// How many rules (lines with a pattern) it has.
    pub(crate) fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

/// What [`CodeOwners::lint`] found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Lint {
    /// Rules that match no file: (line, pattern as written).
    pub unmatched: Vec<(usize, String)>,
    /// Files without an owner.
    pub unowned: Vec<String>,
    /// How many files were looked at.
    pub files: usize,
}

/// Every owner of `files`, each once, in the order first met.
pub(crate) fn owners_of<'a>(
    owners: &CodeOwners,
    files: impl IntoIterator<Item = &'a str>,
) -> (Vec<String>, usize) {
    let mut all: Vec<String> = Vec::new();
    let mut unowned = 0;
    for file in files {
        let list = owners.owners(file);
        if list.is_empty() {
            unowned += 1;
        }
        for owner in list {
            if !all.iter().any(|o| o.eq_ignore_ascii_case(&owner)) {
                all.push(owner);
            }
        }
    }
    (all, unowned)
}

/// An owner as a row shows it: a team by its name (`@acme/payments` → `payments`), anyone
/// else as written.
pub(crate) fn short_owner(owner: &str) -> &str {
    match owner.strip_prefix('@') {
        Some(rest) if !rest.starts_with('@') => rest.rsplit('/').next().unwrap_or(rest),
        _ => owner,
    }
}

/// `a`, `a and b`, `a, b and c`.
pub(crate) fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Someone the forge can be asked to review a pull request: an account, or a team.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reviewer {
    /// `ann`, or `acme/payments` for a team.
    pub name: String,
    pub team: bool,
}

impl Reviewer {
    /// The reviewer an owner of the file stands for, if one can be asked: not an email, nor a
    /// GitLab role (`@@maintainer`).
    pub(crate) fn of(owner: &str) -> Option<Reviewer> {
        let name = owner.strip_prefix('@')?;
        if name.is_empty() || name.starts_with('@') {
            return None;
        }
        Some(Reviewer {
            name: name.to_owned(),
            team: name.contains('/'),
        })
    }

    /// As the dialog shows it.
    pub(crate) fn label(&self) -> String {
        format!("@{}", self.name)
    }
}

/// The reviewers to suggest for a change to `files`: their owners who can be asked.
pub(crate) fn reviewers_for<'a>(
    owners: &CodeOwners,
    files: impl IntoIterator<Item = &'a str>,
) -> Vec<Reviewer> {
    owners_of(owners, files)
        .0
        .iter()
        .filter_map(|o| Reviewer::of(o))
        .collect()
}
