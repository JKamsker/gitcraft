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
const TREE_BYTES: usize = 64 * 1024 * 1024;
const TREE_WORK: usize = 256 * 1024 * 1024;
fn tree_rows(
    data: &Growth,
    cursor: usize,
    active: &dyn Fn() -> bool,
) -> Result<Vec<TreeRow>, String> {
    tree_rows_budget(data, cursor, active, TREE_BYTES, TREE_WORK)
}
fn tree_rows_budget<'a>(
    data: &'a Growth,
    cursor: usize,
    active: &dyn Fn() -> bool,
    mut bytes: usize,
    mut work: usize,
) -> Result<Vec<TreeRow>, String> {
    let files = data.files_at_borrowed(cursor, active)?;
    let mut changes = BTreeSet::new();
    if let Some(frame) = data.frames.get(cursor) {
        for (path, _) in &frame.changes {
            if !active() {
                return Err("Frame cancelled".into());
            }
            changes.insert(path.as_slice());
        }
    }
    // Prefix slices borrow the source snapshot until materialization. The budget
    // reserves each final path and conservative set/row overhead before insertion.
    let mut all = BTreeSet::new();
    let mut insert = |path: &'a [u8], directory: bool| -> Result<(), String> {
        if !active() {
            return Err("Frame cancelled".into());
        }
        let effort = path.len() + 1;
        if effort > work {
            return Err("Replay tree expanded work budget exceeded".into());
        }
        work -= effort;
        if !all.contains(&(path, directory)) {
            let cost = path.len() + 256;
            if cost > bytes || all.len() >= 200_000 {
                return Err("Replay tree expanded allocation/entry budget exceeded".into());
            }
            bytes -= cost;
            all.insert((path, directory));
        }
        Ok(())
    };
    for path in files.iter().chain(&changes) {
        insert(path, false)?;
        for (i, b) in path.iter().enumerate() {
            if i % 256 == 0 && !active() {
                return Err("Frame cancelled".into());
            }
            if *b == b'/' {
                insert(&path[..i], true)?;
            }
        }
    }
    let mut entries: Vec<_> = all.into_iter().collect();
    // Component ordering keeps a folder's descendants before its next sibling.
    entries.sort_by(|(a, ad), (b, bd)| {
        a.split(|c| *c == b'/')
            .cmp(b.split(|c| *c == b'/'))
            .then_with(|| bd.cmp(ad))
    });
    let mut rows = Vec::with_capacity(entries.len());
    for (path, directory) in entries {
        if !active() {
            return Err("Frame cancelled".into());
        }
        let changed = !directory && changes.contains(path);
        let removed = changed && !files.contains(path);
        rows.push(TreeRow {
            path: path.to_vec(),
            directory,
            changed,
            removed,
        });
    }
    Ok(rows)
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
                view.frames.queue(move |active| {if !active(){return Err("Frame cancelled".into());} let rows=tree_rows(&snapshot,at,active)?;if !active(){return Err("Frame cancelled".into());}Ok((at,rows))});
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
    fn tree_budget_refuses_small_prefix_amplification_and_observes_cancellation() {
        let data = Growth {
            baseline: [b"aaaa/bbbb/cccc/dddd/eeee".to_vec()].into_iter().collect(),
            frames: Vec::new(),
            more: false,
            shallow: false,
        };
        assert!(
            tree_rows_budget(&data, 0, &|| true, 1000, TREE_WORK)
                .is_err_and(|e| e.contains("allocation/entry budget"))
        );
        assert!(
            tree_rows_budget(&data, 0, &|| true, TREE_BYTES, 20)
                .is_err_and(|e| e.contains("work budget"))
        );
        let calls = std::cell::Cell::new(0);
        assert!(
            tree_rows(&data, 0, &|| {
                calls.set(calls.get() + 1);
                calls.get() < 4
            })
            .is_err_and(|e| e.contains("cancelled"))
        );
    }
    #[test]
    fn component_order_keeps_subtrees_together_and_file_directory_replacement_is_explicit()
    -> Result<(), String> {
        let data = Growth {
            baseline: [b"a-b".to_vec(), b"a/old".to_vec()].into_iter().collect(),
            frames: vec![
                Frame {
                    hash: "base".into(),
                    name: String::new(),
                    date: String::new(),
                    subject: String::new(),
                    changes: Vec::new(),
                },
                Frame {
                    hash: "tip".into(),
                    name: String::new(),
                    date: String::new(),
                    subject: String::new(),
                    changes: vec![(b"a/old".to_vec(), false), (b"a".to_vec(), true)],
                },
            ],
            more: false,
            shallow: false,
        };
        let rows = tree_rows(&data, 0, &|| true)?;
        assert_eq!(
            rows.iter().map(|r| r.path.as_slice()).collect::<Vec<_>>(),
            vec![b"a".as_slice(), b"a/old", b"a-b"]
        );
        let rows = tree_rows(&data, 1, &|| true)?;
        assert_eq!(rows.iter().filter(|r| r.path == b"a").count(), 2);
        assert!(rows.iter().any(|r| r.path == b"a" && r.directory));
        assert!(
            rows.iter()
                .any(|r| r.path == b"a" && !r.directory && r.changed && !r.removed)
        );
        assert!(rows.iter().any(|r| r.path == b"a/old" && r.removed));
        Ok(())
    }
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
        let rows = tree_rows(&data, 1, &|| true).unwrap_or_default();
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
    #[test]
    fn growth_next_frame_and_pause_open_the_displayed_commit() -> Result<(), String> {
        use egui_kittest::{Harness, kittest::Queryable};
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/growth-ui".into(), &ctx);
        let data = Arc::new(Growth {
            baseline: [b"a".to_vec()].into_iter().collect(),
            frames: vec![
                Frame {
                    hash: "a".repeat(40),
                    name: "A".into(),
                    date: "2026-11-06".into(),
                    subject: "base".into(),
                    changes: Vec::new(),
                },
                Frame {
                    hash: "b".repeat(40),
                    name: "B".into(),
                    date: "2026-11-07".into(),
                    subject: "next".into(),
                    changes: vec![(b"b".to_vec(), true)],
                },
            ],
            more: false,
            shallow: false,
        });
        let rows = tree_rows(&data, 0, &|| true)?;
        tab.growth = Some(State {
            data: Some(data),
            rows,
            requested: Some(0),
            ..Default::default()
        });
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1200.0, 900.0))
            .build_ui_state(|ui, tab: &mut RepoTab| tab.growth_window(ui.ctx()), tab);
        harness.run();
        harness.get_by_label("Next commit").click();
        harness.run_steps(2);
        let began = Instant::now();
        while harness
            .state()
            .growth
            .as_ref()
            .is_some_and(|v| v.shown != 1)
        {
            if began.elapsed() > Duration::from_secs(3) {
                return Err("Frame did not finish".into());
            }
            std::thread::sleep(Duration::from_millis(5));
            harness.run_steps(1);
        }
        assert!(
            harness
                .state()
                .growth
                .as_ref()
                .ok_or("growth")?
                .rows
                .iter()
                .any(|r| r.path == b"b" && r.changed)
        );
        harness.get_by_label("Open this commit").click();
        harness.run();
        assert!(harness.state().growth.is_none());
        assert_eq!(harness.state().detail_hash, Some("b".repeat(40)));
        Ok(())
    }
}
