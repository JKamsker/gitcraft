//! The rows the start page's _All repositories_ list shows: the library's repositories (but
//! those hidden) and the Recent ones, one row per checkout, narrowed by a filter and a search.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::identity::PathKey;
use super::model::{Library, Source};
use crate::app::home_filter;

/// Which repositories the list shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::app) enum Show {
    #[default]
    All,
    Recent,
    Nearby,
    Root(u64),
}

/// A row of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct Row {
    pub path: PathBuf,
    /// In the Recent list.
    pub recent: bool,
    /// Found by the library (by these sources); a Recent-only row has none.
    pub sources: Vec<Source>,
    /// No source's last scan saw it.
    pub stale: bool,
}

impl Row {
    /// Whether it can be hidden from the library (it was found by a scan).
    pub fn hideable(&self) -> bool {
        !self.sources.is_empty()
    }

    fn shown_in(&self, show: &Show) -> bool {
        match show {
            Show::All => true,
            Show::Recent => self.recent,
            Show::Nearby => self.sources.iter().any(|s| matches!(s, Source::Nearby(_))),
            Show::Root(id) => self.sources.contains(&Source::Root(*id)),
        }
    }
}

/// Every row, by name then path (case ignored): the library's repositories not hidden, and
/// the Recent ones (also when hidden: hiding is the library's, Recent is the user's own).
pub(in crate::app) fn all(lib: &Library, recent: &[PathBuf]) -> Vec<Row> {
    let hidden: BTreeSet<PathKey> = lib.hidden.iter().map(|h| PathKey::of(h)).collect();
    let mut rows: BTreeMap<PathKey, Row> = BTreeMap::new();
    for entry in &lib.entries {
        let key = PathKey::of(&entry.path);
        if hidden.contains(&key) {
            continue;
        }
        rows.insert(
            key,
            Row {
                path: entry.path.clone(),
                recent: false,
                sources: entry.sources.iter().cloned().collect(),
                stale: entry.stale(),
            },
        );
    }
    for path in recent {
        rows.entry(PathKey::of(path))
            .or_insert_with(|| Row {
                path: path.clone(),
                recent: true,
                sources: Vec::new(),
                stale: false,
            })
            .recent = true;
    }
    let mut rows: Vec<Row> = rows.into_values().collect();
    rows.sort_by_cached_key(|r| (sort_name(&r.path), PathKey::of(&r.path)));
    rows
}

fn sort_name(path: &Path) -> String {
    home_filter::repo_name(path).to_lowercase()
}

/// The rows `show` and `query` leave, best match first for a query.
/// Ranked as the Recent list's search ranks (see `home_filter::filter`).
pub(in crate::app) fn matching(rows: &[Row], show: &Show, query: &str) -> Vec<usize> {
    let shown = (0..rows.len()).filter(|&i| rows[i].shown_in(show));
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return shown.collect();
    }
    let mut ranked: Vec<(u8, usize)> = shown
        .filter_map(|i| home_filter::rank(&rows[i].path, &query).map(|r| (r, i)))
        .collect();
    ranked.sort();
    ranked.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::super::model::{Outcome, ScanResult};
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn library() -> (Library, u64) {
        let mut lib = Library::default();
        let id = lib.add_root(p("/code"), 1);
        let repos: Vec<PathBuf> = (0..30).map(|i| p(&format!("/code/r{i:02}"))).collect();
        lib.finish(
            &Source::Root(id),
            ScanResult {
                repos,
                outcome: Outcome::Complete,
                at: 1,
            },
        );
        lib.note_nearby(p("/work/projects"));
        lib.finish(
            &Source::Nearby(p("/work/projects")),
            ScanResult {
                repos: vec![p("/work/projects/app"), p("/work/projects/tool")],
                outcome: Outcome::Complete,
                at: 1,
            },
        );
        (lib, id)
    }

    /// 30+ repositories are all listed, the 12 Recent ones untouched and merged by path.
    #[test]
    fn the_library_and_recent_share_one_list() {
        let (mut lib, id) = library();
        let recent = vec![p("/work/projects/app"), p("/elsewhere/solo")];
        let rows = all(&lib, &recent);
        assert_eq!(rows.len(), 33);
        let app = rows.iter().find(|r| r.path == p("/work/projects/app"));
        assert!(app.is_some_and(|r| r.recent && r.hideable()));
        let solo = rows.iter().find(|r| r.path == p("/elsewhere/solo"));
        assert!(solo.is_some_and(|r| r.recent && !r.hideable()));
        assert_eq!(matching(&rows, &Show::Recent, "").len(), 2);
        assert_eq!(matching(&rows, &Show::Nearby, "").len(), 2);
        assert_eq!(matching(&rows, &Show::Root(id), "").len(), 30);
        assert_eq!(matching(&rows, &Show::All, "").len(), 33);
        // Search by name and by path, within the filter.
        let tool = matching(&rows, &Show::All, "tool");
        assert_eq!(tool.len(), 1);
        assert_eq!(rows[tool[0]].path, p("/work/projects/tool"));
        assert_eq!(matching(&rows, &Show::Nearby, "projects").len(), 2);
        assert!(matching(&rows, &Show::Root(id), "tool").is_empty());
        // Hidden: gone from the library's rows, still in Recent's.
        lib.hide(Path::new("/work/projects/app"));
        lib.hide(Path::new("/code/r00"));
        let rows = all(&lib, &recent);
        assert_eq!(rows.len(), 32);
        let app = rows.iter().find(|r| r.path == p("/work/projects/app"));
        assert!(app.is_some_and(|r| r.recent && !r.hideable()));
        assert!(!rows.iter().any(|r| r.path == p("/code/r00")));
        assert_eq!(recent.len(), 2, "Recent is only read");
    }
}
