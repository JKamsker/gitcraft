//! Folder treemap; all Git reads and aggregation run on the shared task pool.
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
                view.error = Some("Use YYYY-MM-DD dates, with start on or before end".into());
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
    pub(super) fn atlas_window(&mut self, ctx: &egui::Context) {
        if self
            .atlas
            .as_ref()
            .is_some_and(|v| v.profile != crate::git::profile_config())
        {
            self.atlas = None;
        }
        let Some(view) = &mut self.atlas else {
            return;
        };
        if let Some(result) = view.reader.poll(&self.tx, ctx) {
            match result {
                Ok(data) => {
                    view.data = Some(data);
                    view.zoom = 0;
                    view.page = 0;
                }
                Err(e) => view.error = Some(e),
            }
        }
        let mut open = true;
        let mut refresh = false;
        let mut file = None;
        egui::Window::new("Churn and contributor atlas").id(egui::Id::new(("atlas", &self.path))).open(&mut open).default_size(vec2(880.0,680.0)).resizable(true).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Size"); ui.selectable_value(&mut view.size, Size::Bytes, "Bytes"); ui.selectable_value(&mut view.size, Size::Changes, "Change frequency");
                ui.separator(); ui.label("Colour"); ui.selectable_value(&mut view.colour, Colour::Churn, "Recent churn"); ui.selectable_value(&mut view.colour, Colour::Contributor, "Most frequent contributor");
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Period"); ui.add(egui::TextEdit::singleline(&mut view.from).hint_text("From YYYY-MM-DD").desired_width(145.0)); ui.add(egui::TextEdit::singleline(&mut view.through).hint_text("Through YYYY-MM-DD").desired_width(155.0));
                egui::ComboBox::from_id_salt("atlas-clock").selected_text(if view.clock == Clock::Author { "Author date" } else { "Committer date" }).show_ui(ui, |ui| { ui.selectable_value(&mut view.clock, Clock::Author, "Author date"); ui.selectable_value(&mut view.clock, Clock::Committer, "Committer date"); });
                refresh |= ui.button("Apply / refresh atlas").clicked();
            });
            if view.reader.busy() { ui.label("Analyzing pinned history and file sizes…"); if ui.button("Cancel atlas analysis").clicked() { view.reader.cancel(); } }
            if let Some(error) = &view.error { ui.colored_label(crate::theme::Tokens::get(ctx).danger, error); }
            let Some(data) = &view.data else { return; };
            ui.label(format!("{} of {} scanned unique commits in period · {} reference tips · tree {}{}{}", data.selected, data.history.commits.len(), data.history.tips, data.head.get(..8).unwrap_or(&data.head), if data.history.more { " · partial history" } else { " · all reachable history" }, if data.history.shallow { " · shallow boundary" } else { "" }));
            ui.small(format!("Applied period: {} – {} ({:?}). Churn counts changed paths per commit, including deletions; folder totals count file touches. Zero-byte/deleted entries get a minimum visible area. File bytes are from the pinned HEAD tree.", data.query.from.map(|d|d.to_string()).unwrap_or_else(||"earliest scanned".into()), data.query.through.map(|d|d.to_string()).unwrap_or_else(||"latest scanned".into()), data.query.clock));
            if data.history.more && view.limit < 100_000 && ui.button("Scan more atlas history").clicked() { view.limit = (view.limit * 2).min(100_000); refresh = true; }
            let node = &data.nodes[view.zoom];
            ui.horizontal_wrapped(|ui| {
                if view.zoom != 0 && ui.button("↑ Repository root").clicked() { view.zoom = 0; view.page = 0; }
                ui.label(if node.path.is_empty() { "/".into() } else { String::from_utf8_lossy(&node.path).into_owned() });
                ui.label(format!("{} bytes · {} file changes", node.bytes, node.changes));
                if !node.contributor.is_empty() { ui.label(format!("Most frequent contributor: {}", node.contributor)); }
            });
            let node = &data.nodes[view.zoom];
            if node.children.is_empty() { ui.label("No files in this snapshot."); return; }
            ui.small("Files and historical folders at the same path are separate hotspots. File history uses HEAD for current files or an observed change revision for historical files. Right-click a file for history across analyzed refs; non-UTF-8 paths open that history directly. Long contributor labels are shortened; numbered identities remain separate.");
            // Bound painter and AccessKit work even for a flat tree with 100,000 files.
            let pages = node.children.len().div_ceil(300);
            view.page = view.page.min(pages.saturating_sub(1));
            if pages > 1 { ui.horizontal(|ui| { if ui.button("Previous hotspots").clicked() { view.page = view.page.saturating_sub(1); } ui.label(format!("Hotspot page {} / {pages}", view.page + 1)); if ui.button("Next hotspots").clicked() { view.page = (view.page + 1).min(pages - 1); } }); }
            let ids: Vec<_> = node.children.iter().skip(view.page * 300).take(300).copied().collect();
            let weights: Vec<_> = ids.iter().map(|id| { let n = &data.nodes[*id]; (if view.size == Size::Bytes { n.bytes as f64 } else { n.changes as f64 }).max(1.0) }).collect();
            let (rect,_) = ui.allocate_exact_size(vec2(ui.available_width().max(50.0), ui.available_height().clamp(160.0,500.0)), egui::Sense::hover());
            let mut boxes = Vec::new(); partition(rect, &weights, 0, &mut boxes);
            let max = ids.iter().map(|id| data.nodes[*id].changes).max().unwrap_or(1).max(1) as f32;
            let t = crate::theme::Tokens::get(ctx);
            for (at, rect) in boxes {
                let id = ids[at]; let n = &data.nodes[id]; let rect = rect.shrink(1.5);
                let label = std::str::from_utf8(&n.path).map(std::borrow::Cow::Borrowed).unwrap_or_else(|_| std::borrow::Cow::Owned(n.path.escape_ascii().to_string()));
                let response = ui.interact(rect, ui.id().with(("atlas-node", id)), egui::Sense::click());
                let fill = if view.colour == Colour::Churn { t.accent.gamma_multiply(0.15 + 0.85 * n.changes as f32 / max) } else { t.lane(n.contributor_key).gamma_multiply(0.7) };
                ui.painter().rect_filled(rect,3.0,fill);
                crate::a11y::a11y(&response, egui::WidgetType::Button, format!("{label} ({}), {} bytes, {} changes, most frequent contributor: {}. {}", if n.directory {"folder"} else {"file"}, n.bytes,n.changes,n.contributor,if n.directory {"Zoom into folder"} else if std::str::from_utf8(&n.path).is_ok() {"Open file history at the pinned revision"} else {"Open scanned history for this exact byte path"}));
                if rect.width() > 42.0 && rect.height() > 22.0 { ui.painter().with_clip_rect(rect.shrink(3.0)).text(rect.left_top()+vec2(4.0,4.0),egui::Align2::LEFT_TOP,n.path.rsplit(|b| *b==b'/').next().map(String::from_utf8_lossy).unwrap_or_default(),egui::FontId::proportional(12.0),t.text); }
                let mut scanned = false;
                response.context_menu(|ui| {if !n.directory && ui.button("Scanned file history across analyzed refs").clicked() {scanned = true; ui.close();}});
                let clicked = response.clicked(); response.on_hover_text(format!("{label}\n{} bytes · {} changes\nMost frequent contributor: {}",n.bytes,n.changes,if n.contributor.is_empty() {"No changes in selected period"} else {n.contributor.as_ref()}));
                if scanned || (clicked && !n.directory && std::str::from_utf8(&n.path).is_err()) {
                    view.byte_history = Some(ByteHistory::new(data.history.clone(),n.path.clone(),view.profile.clone(),view.clock));
                } else if clicked { if !n.directory { if let Ok(path) = String::from_utf8(n.path.clone()) {file = n.revision.as_ref().map(|rev| (path,rev.to_string()));} } else { view.zoom = id; view.page=0; } }
            }
        });
        let mut chosen = None;
        if let Some(history) = &mut view.byte_history {
            let (history_open, commit) = history.show(&self.tx, ctx);
            chosen = commit;
            if !history_open {
                view.byte_history = None;
            }
        }
        if let Some(hash) = chosen {
            self.activity_commit(hash, ctx);
            self.atlas = None;
        } else if let Some((path, head)) = file {
            self.open_file_view(path, Some(head), super::file_view::FileTab::History, ctx);
            self.atlas = None;
        } else if !open {
            self.atlas = None;
        } else if refresh {
            self.read_atlas();
        }
    }
}
/// Balanced recursive slice treemap: every positive weight receives exactly one rectangle.
fn partition(rect: Rect, weights: &[f64], start: usize, out: &mut Vec<(usize, Rect)>) {
    if weights.is_empty() {
        return;
    }
    if weights.len() == 1 {
        out.push((start, rect));
        return;
    }
    let total: f64 = weights.iter().sum();
    let mut left = weights[0];
    let mut split = 1;
    while split < weights.len() - 1 && left < total / 2.0 {
        left += weights[split];
        split += 1;
    }
    let fraction = (left / total) as f32;
    let (a, b) = if rect.width() >= rect.height() {
        let x = rect.left() + rect.width() * fraction;
        (
            Rect::from_min_max(rect.min, pos2(x, rect.bottom())),
            Rect::from_min_max(pos2(x, rect.top()), rect.max),
        )
    } else {
        let y = rect.top() + rect.height() * fraction;
        (
            Rect::from_min_max(rect.min, pos2(rect.right(), y)),
            Rect::from_min_max(pos2(rect.left(), y), rect.max),
        )
    };
    partition(a, &weights[..split], start, out);
    partition(b, &weights[split..], start + split, out);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn non_utf8_hotspot_opens_exact_byte_history_and_immutable_commit() -> Result<(), String> {
        use crate::git::activity::{History, Record};
        use egui_kittest::{Harness, kittest::Queryable};
        use std::sync::Arc;
        let day = Day::parse("2026-11-08").ok_or("day")?;
        let history = Arc::new(History {
            commits: [
                (b"f\xff".to_vec(), "touch-ff", "a".repeat(40)),
                (b"f\xfe".to_vec(), "touch-fe", "b".repeat(40)),
            ]
            .into_iter()
            .map(|(path, subject, hash)| Record {
                hash,
                name: "Author".into(),
                email: String::new(),
                author: day,
                committer: day,
                subject: subject.into(),
                paths: [path].into_iter().collect(),
            })
            .collect(),
            tips: 2,
            ..Default::default()
        });
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/byte-atlas".into(), &ctx);
        tab.atlas = Some(State {
            data: Some(Atlas {
                head: "head".into(),
                history,
                nodes: vec![
                    atlas::Node {
                        children: vec![1],
                        directory: true,
                        ..Default::default()
                    },
                    atlas::Node {
                        path: b"f\xff".to_vec(),
                        revision: Some("side".into()),
                        changes: 1,
                        ..Default::default()
                    },
                ],
                selected: 2,
                query: Query::default(),
            }),
            ..Default::default()
        });
        let mut harness = Harness::builder()
            .with_size([1200.0, 1000.0])
            .build_ui_state(|ui, tab: &mut RepoTab| tab.atlas_window(ui.ctx()), tab);
        harness.run();
        harness.get_by_label_contains(r"f\xff (file)").click();
        harness.run_steps(2);
        let start = std::time::Instant::now();
        while harness
            .state()
            .atlas
            .as_ref()
            .and_then(|v| v.byte_history.as_ref())
            .is_none_or(|h| h.rows.is_empty())
        {
            if start.elapsed() > std::time::Duration::from_secs(5) {
                return Err("Byte path history did not finish".into());
            }
            std::thread::yield_now();
            harness.run_steps(1);
        }
        assert_eq!(
            harness
                .state()
                .atlas
                .as_ref()
                .and_then(|v| v.byte_history.as_ref())
                .map(|h| h.rows.as_slice()),
            Some([0].as_slice())
        );
        harness.get_by_label_contains("touch-ff").click();
        harness.run_steps(2);
        assert!(harness.state().atlas.is_none());
        assert_eq!(harness.state().detail_hash, Some("a".repeat(40)));
        Ok(())
    }
    #[test]
    fn overlapping_file_and_historical_folder_both_remain_reachable() -> Result<(), String> {
        use egui_kittest::{Harness, kittest::Queryable};
        for folder in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = RepoTab::new_deferred("/nonexistent/atlas-overlap".into(), &ctx);
            tab.atlas = Some(State {
                size: Size::Changes,
                data: Some(Atlas {
                    head: "head".into(),
                    history: std::sync::Arc::new(crate::git::activity::History::default()),
                    selected: 2,
                    query: Query::default(),
                    nodes: vec![
                        atlas::Node {
                            directory: true,
                            children: vec![1, 2],
                            ..Default::default()
                        },
                        atlas::Node {
                            path: b"a".to_vec(),
                            bytes: 100,
                            changes: 1,
                            revision: Some("head".into()),
                            ..Default::default()
                        },
                        atlas::Node {
                            path: b"a".to_vec(),
                            directory: true,
                            children: vec![3],
                            changes: 1,
                            ..Default::default()
                        },
                        atlas::Node {
                            path: b"a/old".to_vec(),
                            changes: 1,
                            revision: Some("side".into()),
                            ..Default::default()
                        },
                    ],
                }),
                ..Default::default()
            });
            let mut harness = Harness::builder()
                .with_size(vec2(1200.0, 900.0))
                .build_ui_state(|ui, tab: &mut RepoTab| tab.atlas_window(ui.ctx()), tab);
            harness.run();
            harness
                .get_by_label_contains(if folder { "a (folder)" } else { "a (file)" })
                .click();
            harness.run();
            if folder {
                assert_eq!(harness.state().atlas.as_ref().ok_or("folder")?.zoom, 2);
                harness.get_by_label_contains("a/old (file)").click();
                harness.run();
            }
            let file = harness.state().file_view.as_ref().ok_or("file history")?;
            assert_eq!(file.path, if folder { "a/old" } else { "a" });
            assert_eq!(
                file.rev.as_deref(),
                Some(if folder { "side" } else { "head" })
            );
        }
        Ok(())
    }
    #[test]
    fn treemap_covers_area_once_and_keeps_weights() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, vec2(100.0, 100.0));
        let mut out = Vec::new();
        partition(rect, &[1.0, 3.0, 6.0], 0, &mut out);
        assert_eq!(out.len(), 3);
        for (i, r) in &out {
            assert!((r.area() / rect.area() - [0.1, 0.3, 0.6][*i]).abs() < 0.001);
        }
        assert!(!out[0].1.intersects(out[2].1.shrink(0.01)));
    }
    #[test]
    fn atlas_folder_hotspot_zooms_and_file_hotspot_opens_pinned_history() -> Result<(), String> {
        use egui_kittest::{Harness, kittest::Queryable};
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/atlas-ui".into(), &ctx);
        let nodes = vec![
            atlas::Node {
                children: vec![1],
                bytes: 4,
                ..Default::default()
            },
            atlas::Node {
                path: b"src".to_vec(),
                children: vec![2],
                directory: true,
                bytes: 4,
                ..Default::default()
            },
            atlas::Node {
                path: b"src/a".to_vec(),
                revision: Some("a".repeat(40).into()),
                bytes: 4,
                ..Default::default()
            },
        ];
        tab.atlas = Some(State {
            data: Some(Atlas {
                head: "a".repeat(40),
                history: std::sync::Arc::new(crate::git::activity::History::default()),
                nodes,
                selected: 0,
                query: Query::default(),
            }),
            ..Default::default()
        });
        let mut harness = Harness::builder()
            .with_size(vec2(1200.0, 900.0))
            .build_ui_state(|ui, tab: &mut RepoTab| tab.atlas_window(ui.ctx()), tab);
        harness.run();
        harness
            .get_by_label_contains("src (folder), 4 bytes")
            .click();
        harness.run();
        assert_eq!(harness.state().atlas.as_ref().ok_or("atlas")?.zoom, 1);
        harness
            .get_by_label_contains("src/a (file), 4 bytes")
            .click();
        harness.run();
        assert!(harness.state().atlas.is_none());
        let file = harness.state().file_view.as_ref().ok_or("file history")?;
        assert_eq!(file.path, "src/a");
        assert_eq!(file.rev, Some("a".repeat(40)));
        Ok(())
    }
}
