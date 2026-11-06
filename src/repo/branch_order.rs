//! The order of the local branches in the sidebar: pinned ones at the top, the others by name
//! or by their last commit, newest first. Kept per repository with the app.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::git::Branch;

/// How the local branches are listed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BranchOrder {
    /// Branch names kept at the top, by name.
    pub pinned: BTreeSet<String>,
    /// The others by their last commit, newest first, instead of by name.
    pub by_date: bool,
}

fn id(repo: &Path) -> egui::Id {
    egui::Id::new(("branch-order", repo))
}

impl BranchOrder {
    /// What is kept for the repository `repo`.
    pub fn load(ctx: &egui::Context, repo: &Path) -> Self {
        ctx.data_mut(|d| d.get_persisted::<BranchOrder>(id(repo)))
            .unwrap_or_default()
    }

    pub fn store(&self, ctx: &egui::Context, repo: &Path) {
        ctx.data_mut(|d| d.insert_persisted(id(repo), self.clone()));
    }

    /// Pins `name`, or unpins it if it is pinned.
    pub fn toggle_pin(&mut self, name: &str) {
        if !self.pinned.remove(name) {
            self.pinned.insert(name.to_owned());
        }
    }

    pub fn is_pinned(&self, name: &str) -> bool {
        self.pinned.contains(name)
    }

    /// `branches` (in name order) as the sidebar lists them: the pinned ones, by name, then the
    /// rest, by name or as `by_date` lists them (newest first). Branches `by_date` doesn't know
    /// yet (made since it was read) come after those it does, by name.
    pub fn arrange<'a>(
        &self,
        branches: Vec<&'a Branch>,
        by_date: &[String],
    ) -> (Vec<&'a Branch>, Vec<&'a Branch>) {
        let (pinned, mut rest): (Vec<&Branch>, Vec<&Branch>) = branches
            .into_iter()
            .partition(|b| self.pinned.contains(&b.name));
        if self.by_date {
            let rank: HashMap<&str, usize> = by_date
                .iter()
                .enumerate()
                .map(|(i, n)| (n.as_str(), i))
                .collect();
            // Stable: those of the same rank (unknown ones) stay in name order.
            rest.sort_by_key(|b| rank.get(b.name.as_str()).copied().unwrap_or(usize::MAX));
        }
        (pinned, rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str) -> Branch {
        Branch {
            name: name.to_owned(),
            target: "a".repeat(40),
            is_head: false,
            upstream: String::new(),
            track: String::new(),
        }
    }

    fn names(list: &[&Branch]) -> Vec<String> {
        list.iter().map(|b| b.name.clone()).collect()
    }

    #[test]
    fn pinned_first_then_by_name_or_date() {
        let all = [
            branch("alpha"),
            branch("beta"),
            branch("feature/ünïcode"),
            branch("main"),
            branch("new one"),
        ];
        let refs: Vec<&Branch> = all.iter().collect();
        let dates: Vec<String> = ["beta", "feature/ünïcode", "main", "alpha"]
            .map(str::to_owned)
            .into();
        let mut order = BranchOrder::default();
        order.toggle_pin("main");
        let (pinned, rest) = order.arrange(refs.clone(), &dates);
        assert_eq!(names(&pinned), ["main"]);
        assert_eq!(
            names(&rest),
            ["alpha", "beta", "feature/ünïcode", "new one"]
        );
        order.by_date = true;
        let (_, rest) = order.arrange(refs.clone(), &dates);
        // "new one" isn't dated yet: after the dated ones.
        assert_eq!(
            names(&rest),
            ["beta", "feature/ünïcode", "alpha", "new one"]
        );
        order.toggle_pin("main");
        assert!(!order.is_pinned("main"));
        let (pinned, rest) = order.arrange(refs, &[]);
        assert!(pinned.is_empty());
        assert_eq!(rest.len(), 5);
        let (pinned, rest) = order.arrange(Vec::new(), &dates);
        assert!(pinned.is_empty() && rest.is_empty());
    }

    /// Pins and the sort are kept per repository, and survive the app's restart (serialised
    /// with egui's persisted memory, and read back).
    #[test]
    fn kept_per_repository() {
        let ctx = egui::Context::default();
        let (a, b) = (Path::new("/repos/a"), Path::new("/repos/b"));
        let mut order = BranchOrder::default();
        order.toggle_pin("release/1.0");
        order.by_date = true;
        order.store(&ctx, a);
        assert_eq!(BranchOrder::load(&ctx, a), order);
        assert_eq!(BranchOrder::load(&ctx, b), BranchOrder::default());
        let saved = serde_json::to_string(&order).unwrap_or_default();
        let back: BranchOrder = serde_json::from_str(&saved).unwrap_or_default();
        assert_eq!(back, order);
        // Older saves without a field read with its default.
        let old: BranchOrder = serde_json::from_str(r#"{"by_date": true}"#).unwrap_or_default();
        assert!(old.by_date && old.pinned.is_empty());
    }
}
