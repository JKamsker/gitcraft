//! A real file-tree replay, with bounded asynchronous snapshot reconstruction.
use super::{RepoTab, history_reader::Reader};
use crate::git::growth::{self, Growth};
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};
pub(super) struct State {
    reader: Reader<Arc<Growth>>,
    frames: Reader<(usize, Vec<TreeRow>)>,
    data: Option<Arc<Growth>>,
    rows: Vec<TreeRow>,
    cursor: usize,
    requested: Option<usize>,
    shown: usize,
    playing: bool,
    speed: f32,
    last: Instant,
    error: Option<String>,
    profile: Option<std::path::PathBuf>,
}
struct TreeRow {
    path: Vec<u8>,
    directory: bool,
    changed: bool,
    removed: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            reader: Default::default(),
            frames: Default::default(),
            data: None,
            rows: Vec::new(),
            cursor: 0,
            requested: None,
            shown: 0,
            playing: false,
            speed: 2.0,
            last: Instant::now(),
            error: None,
            profile: crate::git::profile_config(),
        }
    }
}
fn tree_rows(data: &Growth, cursor: usize) -> Vec<TreeRow> {
    let files = data.files_at(cursor);
    let mut all = files.clone();
    let changes = data
        .frames
        .get(cursor)
        .map(|f| {
            f.changes
                .iter()
                .map(|(p, _)| p.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    for path in files.iter().chain(&changes) {
        all.insert(path.clone());
        for (i, b) in path.iter().enumerate() {
            if *b == b'/' {
                all.insert(path[..i].to_vec());
            }
        }
    }
    all.into_iter()
        .map(|path| {
            let changed = changes.contains(&path);
            let removed = changed && !files.contains(&path);
            let directory = !files.contains(&path) && !changes.contains(&path);
            TreeRow {
                path,
                directory,
                changed,
                removed,
            }
        })
        .collect()
}
impl RepoTab {
    pub(super) fn open_growth(&mut self) {
        let mut view = State::default();
        let repo = self.path.clone();
        let profile = view.profile.clone();
        view.reader.queue(move |owned| {
            growth::read(&repo, &|| {
                owned() && crate::git::profile_config() == profile
            })
            .map(Arc::new)
        });
        self.growth = Some(view);
    }
    pub(super) fn growth_window(&mut self, ctx: &egui::Context) {
        if self
            .growth
            .as_ref()
            .is_some_and(|v| v.profile != crate::git::profile_config())
        {
            self.growth = None;
        }
        let Some(view) = &mut self.growth else {
            return;
        };
        if let Some(result) = view.reader.poll(&self.tx, ctx) {
            match result {
                Ok(data) => {
                    view.data = Some(data);
                    view.requested = None;
                }
                Err(e) => view.error = Some(e),
            }
        }
        if let Some(result) = view.frames.poll(&self.tx, ctx) {
            match result {
                Ok((at, rows)) => {
                    view.shown = at;
                    view.rows = rows;
                }
                Err(e) => {
                    view.error = Some(e);
                    view.playing = false;
                }
            }
        }
        let mut open = true;
        let mut commit = None;
        egui::Window::new("Watch the repository grow").id(egui::Id::new(("growth",&self.path))).open(&mut open).default_size(egui::vec2(820.0,650.0)).show(ctx,|ui| {
            if view.reader.busy() {ui.label("Reading pinned first-parent history and baseline tree…");}
            if let Some(error)=&view.error {ui.colored_label(crate::theme::Tokens::get(ctx).danger,error);}
            let Some(data)=&view.data else {return;};
            ui.label(format!("{} first-parent commits of HEAD · {}{}",data.frames.len(),if data.more {"latest 1,000 commits (partial history)"} else {"all available first-parent history"},if data.shallow {" · shallow boundary"} else {""}));
            ui.small("The first frame is a full pinned tree. Later frames apply each commit, including merges against their first parent. Deleted files remain crossed out for their deletion frame. Branch histories are not interleaved.");
            if data.frames.is_empty() {ui.label("No commits yet.");return;}
            let reduced=crate::a11y::motion::reduced(ctx);
            if reduced {view.playing=false;ui.label("Reduced motion is enabled. Scrub or step through static frames.");}
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!reduced,egui::Button::new(if view.playing {"Pause growth"} else {"Play growth"})).clicked() {view.playing = !view.playing;view.last=Instant::now();}
                if ui.button("Previous commit").clicked() {view.cursor=view.cursor.saturating_sub(1);view.playing=false;}
                if ui.button("Next commit").clicked() {view.cursor=(view.cursor+1).min(data.frames.len()-1);view.playing=false;}
                ui.add(egui::Slider::new(&mut view.speed,0.5..=8.0).text("commits / second"));
            });
            if ui.add(egui::Slider::new(&mut view.cursor,0..=data.frames.len()-1).text("History frame")).changed() {view.playing=false;}
            if view.playing && !view.frames.busy() && view.shown==view.cursor && view.last.elapsed()>=Duration::from_secs_f32(1.0/view.speed) {
                if view.cursor+1<data.frames.len() {view.cursor+=1;view.last=Instant::now();} else {view.playing=false;}
            }
            if view.playing {ctx.request_repaint_after(Duration::from_millis(30));}
            if view.requested!=Some(view.cursor) {
                let at=view.cursor;let snapshot=data.clone();view.requested=Some(at);
                view.frames.queue(move |active| {if !active(){return Err("Frame cancelled".into());} let rows=tree_rows(&snapshot,at);if !active(){return Err("Frame cancelled".into());}Ok((at,rows))});
            }
            if view.frames.busy() {ui.label(format!("Preparing frame {}…",view.cursor+1));}
            let frame=&data.frames[view.shown.min(data.frames.len()-1)];
            let t=crate::theme::Tokens::get(ctx);
            let colour=t.lane(frame.name.bytes().fold(0usize,|h,b|h.wrapping_mul(31).wrapping_add(b as usize)));
            ui.colored_label(colour,format!("{} · {} · {}",frame.date,frame.name,frame.subject));
            if ui.add_enabled(!view.playing && !view.frames.busy(),egui::Button::new("Open this commit")).clicked() {commit=Some(frame.hash.clone());}
            ui.label(format!("{} tree entries · {} paths changed in this frame",view.rows.len(),frame.changes.len()));
            egui::ScrollArea::vertical().max_height(440.0).show_rows(ui,22.0,view.rows.len(),|ui,range| {
                for row in &view.rows[range] {
                    let depth=row.path.iter().filter(|b|**b==b'/').count().min(32);
                    let name=row.path.rsplit(|b|*b==b'/').next().unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.add_space(depth as f32*14.0);
                        let name=format!("{} {}",if row.directory {"▸"} else if row.removed {"−"} else if row.changed {"●"} else {"·"},String::from_utf8_lossy(name));
                        let text=egui::RichText::new(name).color(if row.removed {t.danger} else if row.changed {colour} else {t.text_dim});
                        let text=if row.removed {text.strikethrough()} else {text};
                        ui.label(text).on_hover_text(format!("{}{}",String::from_utf8_lossy(&row.path),if row.changed {format!("\n{} worked here at this commit",frame.name)} else {String::new()}));
                    });
                }
            });
        });
        if let Some(hash) = commit {
            self.activity_commit(hash, ctx);
            self.growth = None;
        } else if !open {
            self.growth = None;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::growth::Frame;
    #[test]
    fn real_tree_rows_keep_folders_and_deleted_contributor_hotspots() {
        let data = Growth {
            baseline: [b"src/old".to_vec()].into_iter().collect(),
            frames: vec![
                Frame {
                    hash: "a".into(),
                    name: "A".into(),
                    date: "date".into(),
                    subject: "base".into(),
                    changes: Vec::new(),
                },
                Frame {
                    hash: "b".into(),
                    name: "B".into(),
                    date: "date".into(),
                    subject: "move".into(),
                    changes: vec![(b"src/old".to_vec(), false), (b"src/new".to_vec(), true)],
                },
            ],
            more: false,
            shallow: false,
        };
        let rows = tree_rows(&data, 1);
        assert!(rows.iter().any(|r| r.path == b"src" && r.directory));
        assert!(
            rows.iter()
                .any(|r| r.path == b"src/old" && r.removed && r.changed)
        );
        assert!(
            rows.iter()
                .any(|r| r.path == b"src/new" && r.changed && !r.removed)
        );
    }
}
