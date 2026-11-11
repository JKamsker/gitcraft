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
                return Err(crate::i18n::text("repo-core-2771").into());
            }
            changes.insert(path.as_slice());
        }
    }
    // Prefix slices borrow the source snapshot until materialization. The budget
    // reserves each final path and conservative set/row overhead before insertion.
    let mut all = BTreeSet::new();
    let mut insert = |path: &'a [u8], directory: bool| -> Result<(), String> {
        if !active() {
            return Err(crate::i18n::text("repo-core-2771").into());
        }
        let effort = path.len() + 1;
        if effort > work {
            return Err(crate::i18n::text("repo-core-2772").into());
        }
        work -= effort;
        if !all.contains(&(path, directory)) {
            let cost = path.len() + 256;
            if cost > bytes || all.len() >= 200_000 {
                return Err(crate::i18n::text("repo-core-2773").into());
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
                return Err(crate::i18n::text("repo-core-2771").into());
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
            return Err(crate::i18n::text("repo-core-2771").into());
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
        egui::Window::new(crate::i18n::text("repo-core-2774"))
            .id(egui::Id::new(("growth", &self.path)))
            .open(&mut open)
            .default_size(egui::vec2(820.0, 650.0))
            .show(ctx, |ui| {
                if view.reader.busy() {
                    ui.label(crate::i18n::text("repo-core-2776"));
                }
                if let Some(error) = &view.error {
                    ui.colored_label(crate::theme::Tokens::get(ctx).danger, error);
                }
                let Some(data) = &view.data else {
                    return;
                };
                ui.label(crate::i18n::message(
                    "repo-core-2777",
                    &[
                        ("count", (data.frames.len()).into()),
                        (
                            "history",
                            (if data.more {
                                crate::i18n::text("repo-core-2778")
                            } else {
                                crate::i18n::text("repo-core-2779")
                            })
                            .to_string()
                            .into(),
                        ),
                        (
                            "else",
                            (if data.shallow {
                                crate::i18n::text("repo-core-2780")
                            } else {
                                ""
                            })
                            .to_string()
                            .into(),
                        ),
                    ],
                ));
                ui.small(crate::i18n::text("repo-core-2781"));
                if data.frames.is_empty() {
                    ui.label(crate::i18n::text("repo-core-2782"));
                    return;
                }
                let reduced = crate::a11y::motion::reduced(ctx);
                if reduced {
                    view.playing = false;
                    ui.label(crate::i18n::text("repo-core-2783"));
                }
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            !reduced,
                            egui::Button::new(if view.playing {
                                crate::i18n::text("repo-core-2784")
                            } else {
                                crate::i18n::text("repo-core-2785")
                            }),
                        )
                        .clicked()
                    {
                        view.playing = !view.playing;
                        view.last = Instant::now();
                    }
                    if ui.button(crate::i18n::text("repo-core-2786")).clicked() {
                        view.cursor = view.cursor.saturating_sub(1);
                        view.playing = false;
                    }
                    if ui.button(crate::i18n::text("repo-core-2787")).clicked() {
                        view.cursor = (view.cursor + 1).min(data.frames.len() - 1);
                        view.playing = false;
                    }
                    ui.add(
                        egui::Slider::new(&mut view.speed, 0.5..=8.0)
                            .text(crate::i18n::text("repo-core-2788")),
                    );
                });
                if ui
                    .add(
                        egui::Slider::new(&mut view.cursor, 0..=data.frames.len() - 1)
                            .text(crate::i18n::text("repo-core-2789")),
                    )
                    .changed()
                {
                    view.playing = false;
                }
                if view.playing
                    && !view.frames.busy()
                    && view.shown == view.cursor
                    && view.last.elapsed() >= Duration::from_secs_f32(1.0 / view.speed)
                {
                    if view.cursor + 1 < data.frames.len() {
                        view.cursor += 1;
                        view.last = Instant::now();
                    } else {
                        view.playing = false;
                    }
                }
                if view.playing {
                    ctx.request_repaint_after(Duration::from_millis(30));
                }
                if view.requested != Some(view.cursor) {
                    let at = view.cursor;
                    let snapshot = data.clone();
                    view.requested = Some(at);
                    view.frames.queue(move |active| {
                        if !active() {
                            return Err(crate::i18n::text("repo-core-2771").into());
                        }
                        let rows = tree_rows(&snapshot, at, active)?;
                        if !active() {
                            return Err(crate::i18n::text("repo-core-2771").into());
                        }
                        Ok((at, rows))
                    });
                }
                if view.frames.busy() {
                    ui.label(crate::i18n::message(
                        "repo-core-2790",
                        &[("cursor", format!("{}", view.cursor + 1).into())],
                    ));
                }
                let frame = &data.frames[view.shown.min(data.frames.len() - 1)];
                let t = crate::theme::Tokens::get(ctx);
                let colour = t.lane(
                    frame
                        .name
                        .bytes()
                        .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize)),
                );
                ui.colored_label(
                    colour,
                    format!("{} · {} · {}", frame.date, frame.name, frame.subject),
                );
                if ui
                    .add_enabled(
                        !view.playing && !view.frames.busy(),
                        egui::Button::new(crate::i18n::text("repo-core-2791")),
                    )
                    .clicked()
                {
                    commit = Some(frame.hash.clone());
                }
                ui.label(crate::i18n::message(
                    "repo-core-2792",
                    &[
                        ("count", (view.rows.len()).into()),
                        ("count2", (frame.changes.len()).into()),
                    ],
                ));
                egui::ScrollArea::vertical().max_height(440.0).show_rows(
                    ui,
                    22.0,
                    view.rows.len(),
                    |ui, range| {
                        for row in &view.rows[range] {
                            let depth = row.path.iter().filter(|b| **b == b'/').count().min(32);
                            let name = row.path.rsplit(|b| *b == b'/').next().unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.add_space(depth as f32 * 14.0);
                                let name = format!(
                                    "{} {}",
                                    if row.directory {
                                        "▸"
                                    } else if row.removed {
                                        "−"
                                    } else if row.changed {
                                        "●"
                                    } else {
                                        "·"
                                    },
                                    String::from_utf8_lossy(name)
                                );
                                let text = egui::RichText::new(name).color(if row.removed {
                                    t.danger
                                } else if row.changed {
                                    colour
                                } else {
                                    t.text_dim
                                });
                                let text = if row.removed {
                                    text.strikethrough()
                                } else {
                                    text
                                };
                                ui.label(text).on_hover_text(format!(
                                    "{}{}",
                                    String::from_utf8_lossy(&row.path),
                                    if row.changed {
                                        crate::i18n::message(
                                            "repo-core-2793",
                                            &[("name", frame.name.to_string().into())],
                                        )
                                    } else {
                                        String::new()
                                    }
                                ));
                            });
                        }
                    },
                );
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
#[path = "direct_tests/growth_tests.rs"]
mod tests;
