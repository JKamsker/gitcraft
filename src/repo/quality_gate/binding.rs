//! A repository's Sonar project binding: a per-repository preference stored with the app state
//! (like the primary forge remote), so several repositories may bind one project.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::analysis::{Query, Target};
use crate::forge::PullRequest;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Binding {
    pub project: String,
    /// `local=sonar` pairs, comma separated, for branches named differently on the server.
    pub branches: String,
    /// Show the pull request's analysis when the checked-out branch has an open one.
    pub pull_requests: bool,
    /// The connection it was made for ([`crate::analysis::sonar::Account::identity`]).
    pub connection: String,
}

impl Default for Binding {
    fn default() -> Self {
        Self {
            project: String::new(),
            branches: String::new(),
            pull_requests: true,
            connection: String::new(),
        }
    }
}

fn id(repo: &Path) -> egui::Id {
    egui::Id::new(("sonar-project-binding", repo))
}

impl Binding {
    pub(super) fn load(ctx: &egui::Context, repo: &Path) -> Option<Self> {
        ctx.data_mut(|d| d.get_persisted::<Self>(id(repo)))
            .filter(|b| !b.project.is_empty())
    }

    pub(super) fn store(ctx: &egui::Context, repo: &Path, binding: Option<&Self>) {
        ctx.data_mut(|d| match binding {
            Some(binding) => d.insert_persisted(id(repo), binding.clone()),
            None => d.remove::<Self>(id(repo)),
        });
    }

    /// The error is a Fluent message id.
    pub(super) fn validate(&self) -> Result<(), &'static str> {
        if !crate::analysis::sonar::project_key(self.project.trim()) {
            return Err("analysis-binding-project-invalid");
        }
        self.aliases()
            .map(|_| ())
            .ok_or("analysis-binding-branches-invalid")
    }

    fn aliases(&self) -> Option<Vec<(&str, &str)>> {
        self.branches
            .split(',')
            .map(str::trim)
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (local, sonar) = pair.split_once('=')?;
                let (local, sonar) = (local.trim(), sonar.trim());
                let plain = |s: &str| !s.is_empty() && !s.chars().any(char::is_control);
                (plain(local) && plain(sonar)).then_some((local, sonar))
            })
            .collect()
    }

    /// What to ask for the checked-out `branch`; `None` when HEAD is detached or unborn.
    pub(super) fn query(&self, branch: Option<&str>, prs: &[PullRequest]) -> Option<Query> {
        let branch = branch.filter(|b| !b.is_empty())?;
        let pull_request = prs
            .iter()
            .find(|pr| self.pull_requests && !pr.from_fork && pr.branch == branch);
        let target = match pull_request {
            Some(pr) => Target::PullRequest(pr.number.to_string()),
            None => {
                let aliases = self.aliases().unwrap_or_default();
                let name = aliases
                    .iter()
                    .find(|(local, _)| *local == branch)
                    .map_or(branch, |(_, sonar)| *sonar);
                Target::Branch(name.to_owned())
            }
        };
        Some(Query {
            project: self.project.trim().to_owned(),
            target,
        })
    }
}
