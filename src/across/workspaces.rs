//! Workspaces: named groups of repositories (here, in WSL or over SSH), shown on the start page
//! with how each repository stands, and fetched or opened all at once. Kept with the window's
//! other saved state.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The longest name a workspace may have, in characters.
pub const MAX_NAME: usize = 60;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub name: String,
    /// The repositories' top folders, in the order they were added.
    pub repos: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspaces {
    pub list: Vec<Workspace>,
}

impl Workspaces {
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Adds a workspace called `name`; returns its index.
    pub fn create(&mut self, name: &str) -> Result<usize, String> {
        let name = self.checked_name(name, None)?;
        self.list.push(Workspace {
            name,
            repos: Vec::new(),
        });
        Ok(self.list.len() - 1)
    }

    pub fn rename(&mut self, index: usize, name: &str) -> Result<(), String> {
        let name = self.checked_name(name, Some(index))?;
        let ws = self.list.get_mut(index).ok_or("that workspace is gone")?;
        ws.name = name;
        Ok(())
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.list.len() {
            self.list.remove(index);
        }
    }

    /// Adds `repo` to the workspace at `index`; `false` when it was in it already.
    pub fn add_repo(&mut self, index: usize, repo: &Path) -> bool {
        let Some(ws) = self.list.get(index) else {
            return false;
        };
        if ws.repos.iter().any(|r| same(r, repo)) {
            return false;
        }
        let spelled = self.spelling_of(repo);
        if let Some(ws) = self.list.get_mut(index) {
            ws.repos.push(spelled);
        }
        true
    }

    pub fn remove_repo(&mut self, index: usize, repo: &Path) {
        if let Some(ws) = self.list.get_mut(index) {
            ws.repos.retain(|r| !same(r, repo));
        }
    }

    /// Every repository of every workspace, each once, in the order they first appear. Plain
    /// comparison: [`Workspaces::add_repo`] already keeps one spelling of each.
    pub fn all_repos(&self) -> Vec<PathBuf> {
        let mut all: Vec<PathBuf> = Vec::new();
        for repo in self.list.iter().flat_map(|w| &w.repos) {
            if !all.contains(repo) {
                all.push(repo.clone());
            }
        }
        all
    }

    /// The spelling `repo` already has in any workspace, so that one repository is one path
    /// everywhere; `repo` itself when it is new.
    pub fn spelling_of(&self, repo: &Path) -> PathBuf {
        self.list
            .iter()
            .flat_map(|w| &w.repos)
            .find(|r| same(r, repo))
            .cloned()
            .unwrap_or_else(|| repo.to_owned())
    }

    /// `name` trimmed, if it can be a workspace's (that isn't the one at `except`).
    fn checked_name(&self, name: &str, except: Option<usize>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Give the workspace a name.".to_owned());
        }
        if name.chars().count() > MAX_NAME {
            return Err(format!(
                "A workspace's name can be at most {MAX_NAME} characters long."
            ));
        }
        if name.chars().any(char::is_control) {
            return Err("A workspace's name can't have line breaks or tabs in it.".to_owned());
        }
        let taken = self
            .list
            .iter()
            .enumerate()
            .any(|(i, w)| Some(i) != except && w.name.to_lowercase() == name.to_lowercase());
        if taken {
            return Err(format!("There is a workspace called “{name}” already."));
        }
        Ok(name.to_owned())
    }
}

/// Whether two paths are the same repository: equal, the same folder of the same other machine
/// however written (`\\wsl$\…` or `\\wsl.localhost\…`), or the same folder here. Only when a
/// repository is added: it reads the disk.
fn same(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (crate::remote::parse(a), crate::remote::parse(b)) {
        (Some(x), Some(y)) => x == y,
        (None, None) => crate::remote::same_dir(a, b),
        _ => false,
    }
}

/// Translate only known name-validation diagnostics, preserving raw state and opaque input.
pub fn display_error(raw: &str) -> String {
    let fixed = match raw {
        "that workspace is gone" => Some("interaction-workspace-name-gone"),
        "Give the workspace a name." => Some("interaction-workspace-name-required"),
        "A workspace's name can't have line breaks or tabs in it." => {
            Some("interaction-workspace-name-controls")
        }
        _ => None,
    };
    if let Some(id) = fixed {
        return crate::i18n::text(id).to_owned();
    }
    if raw == format!("A workspace's name can be at most {MAX_NAME} characters long.") {
        return crate::i18n::message(
            "interaction-workspace-name-length",
            &[("limit", MAX_NAME.into())],
        );
    }
    if let Some(name) = raw
        .strip_prefix("There is a workspace called “")
        .and_then(|s| s.strip_suffix("” already."))
    {
        return crate::i18n::message(
            "interaction-workspace-name-duplicate",
            &[("name", name.into())],
        );
    }
    super::display_error(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_unique_and_bounded() {
        let mut ws = Workspaces::default();
        assert_eq!(ws.create("  Wörk 日本 "), Ok(0));
        assert_eq!(ws.list[0].name, "Wörk 日本");
        assert!(
            ws.create("wörk 日本").is_err(),
            "the same name in other case"
        );
        assert!(ws.create("   ").is_err());
        assert!(ws.create("a\nb").is_err());
        assert!(ws.create(&"x".repeat(MAX_NAME + 1)).is_err());
        assert_eq!(ws.create(&"é".repeat(MAX_NAME)), Ok(1));
        assert_eq!(ws.create("Home"), Ok(2));
        assert!(ws.rename(2, "WÖRK 日本").is_err());
        assert_eq!(ws.rename(2, "home "), Ok(()));
        assert_eq!(ws.list[2].name, "home");
        assert!(ws.rename(9, "x").is_err());
    }

    #[test]
    fn repositories_are_kept_once_in_their_order() {
        let mut ws = Workspaces::default();
        let a = ws.create("A").unwrap_or_default();
        let b = ws.create("B").unwrap_or_default();
        let ssh = PathBuf::from("ssh://build/home/me/x y");
        let wsl = PathBuf::from(r"\\wsl.localhost\Ubuntu\home\me\r");
        assert!(ws.add_repo(a, &ssh));
        assert!(!ws.add_repo(a, &ssh));
        assert!(ws.add_repo(a, &wsl));
        assert!(ws.add_repo(b, &ssh));
        // The same folder of the same machine written another way is the same repository,
        // in its first spelling everywhere.
        let ssh_upper = PathBuf::from("SSH://build/home/me//x y/");
        assert!(!ws.add_repo(a, &ssh_upper));
        ws.remove_repo(b, &ssh);
        assert!(ws.add_repo(b, &ssh_upper));
        assert_eq!(ws.list[b].repos.last(), Some(&ssh));
        if cfg!(windows) {
            let wsl_dollar = PathBuf::from(r"\\wsl$\Ubuntu\home\me\r");
            assert!(!ws.add_repo(a, &wsl_dollar));
        }
        assert!(!ws.add_repo(7, &ssh));
        assert_eq!(ws.all_repos(), vec![ssh.clone(), wsl.clone()]);
        ws.remove_repo(a, &ssh);
        assert_eq!(ws.list[a].repos, vec![wsl.clone()]);
        ws.remove(a);
        assert_eq!(ws.list.len(), 1);
        assert_eq!(ws.list[0].name, "B");
        ws.remove(5);
        assert_eq!(ws.list.len(), 1);
    }

    #[test]
    fn survive_saving_and_loading() -> Result<(), String> {
        let mut ws = Workspaces::default();
        let i = ws.create("Clients — ünïcode ✓")?;
        ws.add_repo(i, Path::new("/home/me/with space"));
        ws.add_repo(i, Path::new("ssh://me@host:2222/~/repo"));
        let text = serde_json::to_string(&ws).map_err(|e| e.to_string())?;
        let back: Workspaces = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        assert_eq!(back, ws);
        // Saved before workspaces existed, or with a field from a later version.
        let old: Workspaces = serde_json::from_str("{}").map_err(|e| e.to_string())?;
        assert!(old.is_empty());
        let newer: Workspaces =
            serde_json::from_str(r#"{"list":[{"name":"x","repos":[],"colour":3}],"v":2}"#)
                .map_err(|e| e.to_string())?;
        assert_eq!(newer.list[0].name, "x");
        Ok(())
    }
}
