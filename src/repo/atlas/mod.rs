//! Folder treemap; all Git reads and aggregation run on the shared task pool.
mod treemap;
mod view;
use treemap::*;
mod byte_history;
use super::{RepoTab, history_reader::Reader};
use crate::git::{
    activity::{Clock, Day, Query},
    atlas::{self, Atlas},
};
use byte_history::ByteHistory;
use egui::{Rect, pos2, vec2};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Size {
    Bytes,
    Changes,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Colour {
    Churn,
    Contributor,
}
pub(super) struct State {
    reader: Reader<Atlas>,
    data: Option<Atlas>,
    error: Option<String>,
    from: String,
    through: String,
    clock: Clock,
    size: Size,
    colour: Colour,
    zoom: usize,
    page: usize,
    limit: usize,
    profile: Option<std::path::PathBuf>,
    byte_history: Option<ByteHistory>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            reader: Default::default(),
            data: None,
            error: None,
            from: String::new(),
            through: String::new(),
            clock: Clock::Author,
            size: Size::Bytes,
            colour: Colour::Churn,
            zoom: 0,
            page: 0,
            limit: 5000,
            profile: crate::git::profile_config(),
            byte_history: None,
        }
    }
}
impl RepoTab {
    pub(super) fn open_atlas(&mut self) {
        self.atlas = Some(State::default());
        self.read_atlas();
    }
    fn read_atlas(&mut self) {
        let Some(view) = &mut self.atlas else {
            return;
        };
        let parse = |s: &str| {
            if s.trim().is_empty() {
                Ok(None)
            } else {
                Day::parse(s).map(Some).ok_or("Use YYYY-MM-DD dates")
            }
        };
        let query = match (parse(&view.from), parse(&view.through)) {
            (Ok(from), Ok(through)) if !matches!((from,through),(Some(a),Some(b)) if a>b) => {
                Query {
                    from,
                    through,
                    clock: view.clock,
                }
            }
            _ => {
                view.error = Some(invalid_range_error());
                return;
            }
        };
        let repo = self.path.clone();
        let limit = view.limit;
        let profile = view.profile.clone();
        view.error = None;
        view.reader.queue(move |owned| {
            atlas::read(&repo, limit, query, &|| {
                owned() && crate::git::profile_config() == profile
            })
        });
    }
}
#[cfg(test)]
mod tests;

fn invalid_range_error() -> String {
    "Use YYYY-MM-DD dates, with start on or before end".into()
}

#[cfg(test)]
mod fluent_r1_cache_tests;
