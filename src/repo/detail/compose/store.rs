//! What the commit box keeps between runs: saved message templates (for every repository, or
//! for one), the prefix picker's style and the patterns that turn issue references into links.
//! Kept with the app, not in any repository.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The most templates kept in one list, and the longest one.
pub const MAX_TEMPLATES: usize = 50;
pub const MAX_TEMPLATE_LEN: usize = 20_000;

/// A message saved to start commits from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Template {
    pub name: String,
    pub text: String,
}

/// What the type / scope picker writes in front of the summary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrefixStyle {
    /// No picker.
    Off,
    /// `type(scope): summary`.
    #[default]
    Conventional,
    /// An emoji: `✨ summary`.
    Gitmoji,
}

impl PrefixStyle {
    pub const ALL: [PrefixStyle; 3] = [Self::Off, Self::Conventional, Self::Gitmoji];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Conventional => "Conventional Commits",
            Self::Gitmoji => "Gitmoji",
        }
    }
}

/// Text matching `pattern` (a regular expression) in a message links to `url`, where `$0` is
/// what matched and `$1`, `$2`… its groups.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct IssueRule {
    pub pattern: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    /// Templates offered in every repository.
    pub global: Vec<Template>,
    /// Templates of one repository, by its folder (as [`repo_key`] writes it).
    pub per_repo: BTreeMap<String, Vec<Template>>,
    pub style: PrefixStyle,
    /// Issue links, besides `#12` to the forge's issue 12.
    pub issue_rules: Vec<IssueRule>,
}

/// Where a template is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Global,
    Repo,
}

/// The key a repository's own templates are kept under.
pub fn repo_key(repo: &Path) -> String {
    repo.to_string_lossy().into_owned()
}

fn id() -> egui::Id {
    egui::Id::new("gitcraft-commit-compose")
}

impl Store {
    pub fn load(ctx: &egui::Context) -> Store {
        ctx.data_mut(|d| d.get_persisted::<Store>(id()))
            .unwrap_or_default()
    }

    pub fn save(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_persisted(id(), self.clone()));
    }

    /// The templates of `scope` for the repository `repo`.
    pub fn templates(&self, scope: Scope, repo: &Path) -> &[Template] {
        match scope {
            Scope::Global => &self.global,
            Scope::Repo => self
                .per_repo
                .get(&repo_key(repo))
                .map_or(&[], Vec::as_slice),
        }
    }

    fn templates_mut(&mut self, scope: Scope, repo: &Path) -> &mut Vec<Template> {
        match scope {
            Scope::Global => &mut self.global,
            Scope::Repo => self.per_repo.entry(repo_key(repo)).or_default(),
        }
    }

    /// Saves `text` as the template `name` of `scope`, replacing one of that name. Refused,
    /// with why, when either is empty or too long, or the list is full.
    pub fn save_template(
        &mut self,
        scope: Scope,
        repo: &Path,
        name: &str,
        text: &str,
    ) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Give the template a name".to_owned());
        }
        if text.trim().is_empty() {
            return Err("Write the message to save as a template first".to_owned());
        }
        if text.chars().count() > MAX_TEMPLATE_LEN {
            return Err(format!(
                "A template can be at most {MAX_TEMPLATE_LEN} characters"
            ));
        }
        let list = self.templates_mut(scope, repo);
        let template = Template {
            name: name.to_owned(),
            text: text.trim_end().to_owned(),
        };
        if let Some(same) = list.iter_mut().find(|t| t.name == name) {
            *same = template;
            return Ok(());
        }
        if list.len() >= MAX_TEMPLATES {
            return Err(format!(
                "There are already {MAX_TEMPLATES} templates here; delete one first"
            ));
        }
        list.push(template);
        list.sort_by_key(|t| t.name.to_lowercase());
        Ok(())
    }

    pub fn delete_template(&mut self, scope: Scope, repo: &Path, name: &str) {
        let list = self.templates_mut(scope, repo);
        list.retain(|t| t.name != name);
        if scope == Scope::Repo && list.is_empty() {
            self.per_repo.remove(&repo_key(repo));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_kept_per_scope() -> Result<(), String> {
        let mut store = Store::default();
        let (a, b) = (Path::new("/r/a"), Path::new("/r/b b"));
        store.save_template(Scope::Global, a, "Bug", "fix: \n\nRefs: ")?;
        store.save_template(Scope::Repo, a, " Release ", "chore(release): v")?;
        assert_eq!(store.templates(Scope::Global, b).len(), 1);
        assert_eq!(store.templates(Scope::Repo, a)[0].name, "Release");
        assert!(store.templates(Scope::Repo, b).is_empty());
        // The same name replaces it.
        store.save_template(Scope::Repo, a, "Release", "chore: release ünïcode")?;
        assert_eq!(store.templates(Scope::Repo, a).len(), 1);
        assert_eq!(
            store.templates(Scope::Repo, a)[0].text,
            "chore: release ünïcode"
        );
        store.delete_template(Scope::Repo, a, "Release");
        assert!(store.per_repo.is_empty());
        Ok(())
    }

    #[test]
    fn bad_templates_are_refused_with_why() {
        let mut store = Store::default();
        let r = Path::new("/r");
        assert!(store.save_template(Scope::Global, r, " ", "x").is_err());
        assert!(store.save_template(Scope::Global, r, "x", "  \n").is_err());
        let huge = "x".repeat(MAX_TEMPLATE_LEN + 1);
        assert!(store.save_template(Scope::Global, r, "x", &huge).is_err());
        for i in 0..MAX_TEMPLATES {
            assert!(
                store
                    .save_template(Scope::Global, r, &format!("t{i}"), "x")
                    .is_ok()
            );
        }
        let full = store.save_template(Scope::Global, r, "one more", "x");
        assert!(full.is_err_and(|e| e.contains("delete one")));
        // Replacing one still works when full.
        assert!(store.save_template(Scope::Global, r, "t3", "y").is_ok());
    }

    #[test]
    fn what_is_kept_reads_back() -> Result<(), String> {
        let mut store = Store {
            style: PrefixStyle::Gitmoji,
            ..Store::default()
        };
        store.issue_rules.push(IssueRule {
            pattern: r"JIRA-\d+".into(),
            url: "https://jira.example.com/browse/$0".into(),
        });
        let json = serde_json::to_string(&store).map_err(|e| e.to_string())?;
        let back: Store = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        assert_eq!(back, store);
        // What an older version kept (nothing of this) reads as the defaults.
        let old: Store = serde_json::from_str("{}").map_err(|e| e.to_string())?;
        assert_eq!(old.style, PrefixStyle::Conventional);
        Ok(())
    }
}
