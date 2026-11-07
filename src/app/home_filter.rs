//! Filtering the start page's recent repositories by what is typed, and the row picked with
//! ↑ / ↓ among those left.

use std::path::{Path, PathBuf};

/// The id of the start page's search field.
pub(super) fn search_id() -> egui::Id {
    egui::Id::new("home-recent-filter")
}

/// Whether the start page's search field has the keyboard (the window's shortcuts still
/// work then: it only takes typing, ↑ / ↓, Enter and Esc).
pub(super) fn search_has_focus(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.has_focus(search_id()))
}

/// A recent repository's name as the list shows it: its folder's name (for one on another
/// machine, the last name of its path there).
pub(super) fn repo_name(path: &Path) -> String {
    if let Some(target) = crate::remote::parse(path) {
        return target
            .path
            .rsplit('/')
            .find(|s| !s.is_empty())
            .unwrap_or(&target.path)
            .to_owned();
    }
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// How well `path` matches `query` (already lower-cased, trimmed): lower is better, `None` for
/// no match. A name that starts with it comes first, then a name that contains it, then a path
/// that does, then a name whose letters come in its order (never a path so: almost every path
/// has the letters of a short query somewhere).
fn rank(path: &Path, query: &str) -> Option<u8> {
    let name = repo_name(path).to_lowercase();
    let full = crate::remote::display(path).to_lowercase();
    if name.starts_with(query) {
        Some(0)
    } else if name.contains(query) {
        Some(1)
    } else if full.contains(query) {
        Some(2)
    } else if super::palette::fuzzy(&name, query) {
        Some(3)
    } else {
        None
    }
}

/// The indexes in `recent` of the repositories matching `query`, best first (in their own
/// order among equals); all of them for an empty query. Case does not matter.
pub(super) fn filter(recent: &[PathBuf], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return (0..recent.len()).collect();
    }
    let mut ranked: Vec<(u8, usize)> = recent
        .iter()
        .enumerate()
        .filter_map(|(i, p)| rank(p, &query).map(|r| (r, i)))
        .collect();
    ranked.sort();
    ranked.into_iter().map(|(_, i)| i).collect()
}

/// The search and the highlighted row among the matches.
#[derive(Default)]
pub(super) struct RecentFilter {
    pub query: String,
    /// The highlighted row, among the matches.
    pub selected: usize,
}

impl RecentFilter {
    /// Keeps the highlight on a row of `len` (the first when the list shrank under it).
    pub(super) fn clamp(&mut self, len: usize) {
        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }
    }

    /// Moves the highlight by `by` rows among `len`, stopping at either end.
    pub(super) fn step(&mut self, by: isize, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        let at = self.selected.min(len - 1) as isize + by;
        self.selected = at.clamp(0, len as isize - 1) as usize;
    }

    /// The highlighted row's index in the recent list, if any matches.
    pub(super) fn picked(&self, matches: &[usize]) -> Option<usize> {
        matches.get(self.selected).copied()
    }

    /// Esc: clears the search. Returns whether there was one.
    pub(super) fn clear(&mut self) -> bool {
        let had = !self.query.is_empty();
        self.query.clear();
        self.selected = 0;
        had
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn matches_name_then_path_ignoring_case() {
        let recent = paths(&[
            "/work/Backend",
            "/home/me/gitcraft",
            "/src/craft-tools",
            "/projects/web/frontend",
        ]);
        assert_eq!(filter(&recent, ""), vec![0, 1, 2, 3]);
        assert_eq!(filter(&recent, "  "), vec![0, 1, 2, 3]);
        // A name starting with it, then one containing it.
        assert_eq!(filter(&recent, "CRAFT"), vec![2, 1]);
        // Only in the path.
        assert_eq!(filter(&recent, "web"), vec![3]);
        // Letters in order.
        assert_eq!(filter(&recent, "bknd"), vec![0]);
        assert!(filter(&recent, "zzz").is_empty());
        assert!(filter(&[], "x").is_empty());
    }

    /// A short query doesn't match everything through the letters of long paths.
    #[test]
    fn letters_in_order_count_in_the_name_only() {
        let recent = paths(&["/tmp/gitcraft-scan-1234/alpha", "/home/me/projects/beta"]);
        assert_eq!(filter(&recent, "be"), vec![1]);
        assert_eq!(filter(&recent, "aph"), vec![0]);
        assert!(filter(&recent, "mrj").is_empty());
    }

    #[test]
    fn unicode_names_and_spaces() {
        let recent = paths(&["/home/me/Größe Projekt", "/home/me/日本語 repo"]);
        assert_eq!(filter(&recent, "größe"), vec![0]);
        assert_eq!(filter(&recent, "GRÖSSE").len(), 0);
        assert_eq!(filter(&recent, "日本"), vec![1]);
        assert_eq!(filter(&recent, "e p").first(), Some(&0));
    }

    #[test]
    fn remote_names_are_their_last_folder() {
        assert_eq!(repo_name(Path::new("ssh://box/home/me/api")), "api");
        assert_eq!(repo_name(Path::new("/x/y/")), "y");
        let recent = paths(&["ssh://box/home/me/api"]);
        assert_eq!(filter(&recent, "box"), vec![0]);
    }

    #[test]
    fn the_highlight_stays_in_the_list() {
        let mut f = RecentFilter::default();
        f.step(-1, 3);
        assert_eq!(f.selected, 0);
        f.step(1, 3);
        f.step(1, 3);
        f.step(1, 3);
        assert_eq!(f.selected, 2);
        assert_eq!(f.picked(&[4, 5, 6]), Some(6));
        // The list shrank under it.
        f.clamp(1);
        assert_eq!(f.selected, 0);
        f.clamp(0);
        assert_eq!(f.selected, 0);
        assert_eq!(f.picked(&[]), None);
        f.selected = 9;
        f.step(1, 0);
        assert_eq!(f.selected, 0);
        f.selected = 9;
        f.step(-1, 4);
        assert_eq!(f.selected, 2);
        f.query = "x".into();
        assert!(f.clear());
        assert!(!f.clear());
    }
}
