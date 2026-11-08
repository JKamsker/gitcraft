//! A whole file as it is at a commit, in the index or on disk: read, coloured and shown with
//! its line numbers. For the file view's File tab, and diffs switched to the file.

use std::path::Path;

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};

use super::DiffKey;
use super::find::{self, Find, Layout, Seen, Source, SourceKey};
use crate::forge::links::Lines;
pub(crate) type LineMenu<'a> = &'a mut dyn FnMut(&mut Ui, Lines);
use super::highlight::{self, Spans};
use crate::git::{self, Area};
use crate::theme::{self, Tokens};

const LINE_H: f32 = 19.0;
const NUM_W: f32 = 56.0;
/// Bytes looked at for a NUL, which makes a file binary (as git decides it).
const SNIFF: usize = 8000;

pub(crate) struct FileText {
    /// The lines, tabs expanded; empty for a binary file.
    lines: Vec<String>,
    spans: Option<Vec<Spans>>,
    /// Characters in the longest line, for the width to scroll.
    longest: usize,
    /// How big a binary file is, which isn't shown as text.
    binary: Option<usize>,
    /// Read from before the change: the change deleted the file.
    pub(crate) before: bool,
}

impl FileText {
    pub(crate) fn new(bytes: &[u8], path: &str, dark: bool) -> Self {
        if bytes[..bytes.len().min(SNIFF)].contains(&0) {
            return Self {
                lines: Vec::new(),
                spans: None,
                longest: 0,
                binary: Some(bytes.len()),
                before: false,
            };
        }
        let mut lines = split_lines(&String::from_utf8_lossy(bytes));
        let mut spans = highlight::highlight(path, lines.iter().map(String::as_str), dark);
        for (i, line) in lines.iter_mut().enumerate() {
            let _ = highlight::cap_line(line, spans.as_mut().and_then(|s| s.get_mut(i)));
        }
        let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        Self {
            lines,
            spans,
            longest,
            binary: None,
            before: false,
        }
    }
}

/// The lines of `text`, without their line endings and with tabs four spaces wide.
fn split_lines(text: &str) -> Vec<String> {
    text.split_terminator('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).replace('\t', "    "))
        .collect()
}

/// `path` at `rev` (`""`: the index; `None`: on disk), coloured; on a background thread.
pub(crate) fn load(
    repo: &Path,
    rev: Option<&str>,
    path: &str,
    dark: bool,
) -> Result<FileText, String> {
    git::file_at(repo, rev, path).map(|bytes| FileText::new(&bytes, path, dark))
}

/// The file of a diff as it is after the change, or before it when the change deleted it.
pub(crate) fn load_for_diff(repo: &Path, key: &DiffKey, dark: bool) -> Result<FileText, String> {
    let (path, after, before): (&str, Option<String>, Option<Option<String>>) = match key {
        DiffKey::Remerge { hash, path, .. } => (path, Some(hash.clone()), None),
        DiffKey::Commit { hash, path } => {
            (path, Some(hash.clone()), Some(Some(format!("{hash}^"))))
        }
        DiffKey::Work(file) => match file.area {
            Area::Staged => (
                &file.path,
                Some(String::new()),
                Some(Some("HEAD".to_owned())),
            ),
            Area::Unstaged => (&file.path, None, Some(Some(String::new()))),
            Area::Untracked | Area::Conflicted => (&file.path, None, None),
        },
        DiffKey::Range { from, to, path } => (path, to.clone(), Some(Some(from.clone()))),
    };
    match load(repo, after.as_deref(), path, dark) {
        Err(e) => {
            let Some(before) = before else { return Err(e) };
            let mut text = load(repo, before.as_deref(), path, dark).map_err(|_| e)?;
            text.before = true;
            Ok(text)
        }
        ok => ok,
    }
}

/// Lines picked in a file shown: where the pick started and where it ends (0-based).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LinePick {
    anchor: usize,
    cursor: usize,
}

impl LinePick {
    fn range(self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }

    fn contains(self, i: usize) -> bool {
        let (a, b) = self.range();
        (a..=b).contains(&i)
    }

    /// The pick after a click on line `i` (`extend`: with Shift, from where it started).
    fn clicked(pick: Option<Self>, i: usize, extend: bool) -> Option<Self> {
        match pick {
            Some(p) if extend => Some(Self { cursor: i, ..p }),
            Some(p) if p.anchor == i && p.cursor == i => None,
            _ => Some(Self {
                anchor: i,
                cursor: i,
            }),
        }
    }

    /// The lines as git numbers them (from 1).
    fn numbers(self) -> (u32, u32) {
        let (a, b) = self.range();
        let n = |i: usize| u32::try_from(i + 1).unwrap_or(u32::MAX);
        (n(a), n(b))
    }
}

/// The file, or a spinner, the error or a note in its place. `id`: what keeps its scrolling
/// and the lines picked in it (click, Shift+click or drag over them). `history`: why the lines
/// picked can have no history, if they can't. Returns the lines (from 1) whose history was
/// asked for from their right-click menu.
pub(crate) fn focus(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug, line: usize) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(("file-text-focus", id)), line));
}

// The optional logical location accompanies the existing search/history/menu inputs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: Option<&Result<FileText, String>>,
    find: &mut Find,
    source: impl FnOnce() -> Source,
    history: Result<(), &str>,
    mut links: Option<LineMenu<'_>>,
    location: Option<(&std::cell::Cell<Option<usize>>, Option<usize>)>,
) -> Option<(u32, u32)> {
    let t = Tokens::get(ui.ctx());
    let data = match text {
        None => {
            ui.centered_and_justified(|ui| ui.add(crate::spinner::Spinner::new()));
            return None;
        }
        Some(Err(e)) => {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.colored_label(t.danger, e);
            });
            return None;
        }
        Some(Ok(data)) => data,
    };
    let note = match data.binary {
        Some(size) => Some(crate::i18n::message("repo-core-2465", &[("size", (size).into())])),
        None if data.lines.is_empty() => Some(crate::i18n::text("repo-core-2466").to_owned()),
        None => None,
    };
    if let Some(note) = note {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(note).color(t.text_dim));
        });
        return None;
    }
    let pick_id = egui::Id::new(("file_text_pick", &id));
    let mut pick: Option<LinePick> = ui.data(|d| d.get_temp(pick_id)).flatten();
    let free = !crate::a11y::keyboard_taken(ui.ctx());
    if free && crate::keymap::take(ui.ctx(), crate::keymap::Command::SelectionAll) {
        pick = Some(LinePick {
            anchor: 0,
            cursor: data.lines.len() - 1,
        });
        ui.ctx().request_repaint();
    }
    let mut asked = if free
        && history.is_ok()
        && crate::keymap::take(ui.ctx(), crate::keymap::Command::SelectionHistory)
    {
        pick.map(LinePick::numbers)
    } else {
        None
    };
    let extend = ui.input(|i| i.modifiers.shift);
    let drag_id = pick_id.with("drag");
    let (down, pointer_y) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.interact_pos().map(|p| p.y),
        )
    });
    let mut dragging = down && ui.data(|d| d.get_temp::<bool>(drag_id)).unwrap_or(false);
    let font = theme::mono(12.0);
    let char_w = ui
        .painter()
        .layout_no_wrap("M".to_owned(), font.clone(), t.text)
        .size()
        .x;
    let width = (NUM_W + 16.0 + data.longest as f32 * char_w + 24.0).max(ui.available_width());
    ui.spacing_mut().item_spacing.y = 0.0;
    let lines = &data.lines;
    if find.open {
        let key = SourceKey {
            what: source(),
            at: lines.as_ptr() as usize,
            len: lines.len(),
        };
        find.update(
            ui.ctx(),
            key,
            lines.len(),
            |i| find::nth(lines, i),
            Layout::Lines,
        );
    }
    let reveal = ui.data_mut(|d| d.remove_temp::<usize>(egui::Id::new(("file-text-focus", &id))));
    let mut scroll = egui::ScrollArea::both()
        .id_salt(("file_text", id))
        .auto_shrink(false);
    if let Some(line) = reveal {
        scroll = scroll.vertical_scroll_offset(line.saturating_sub(3) as f32 * LINE_H);
    }
    if let Some(hit) = find.take_reveal() {
        scroll = find.scroll_to(
            scroll,
            &hit,
            find::nth(lines, hit.line).unwrap_or_default(),
            char_w,
        );
    }
    if let Some((goto, _)) = location
        && let Some(line) = goto.take()
    {
        if (1..=lines.len()).contains(&line) {
            scroll = scroll.vertical_scroll_offset(
                (line.saturating_sub(1) as f32 * LINE_H - 2.0 * LINE_H).max(0.0),
            );
        } else {
            ui.weak(crate::i18n::text("repo-core-2470"));
        }
    }
    let found = &*find;
    let out = scroll.show_rows(ui, LINE_H, data.lines.len(), |ui, range| {
        for i in range {
            let Some(line) = data.lines.get(i) else {
                continue;
            };
            let (rect, resp) = ui.allocate_exact_size(vec2(width, LINE_H), Sense::click_and_drag());
            crate::a11y::a11y_selected(
                &resp,
                || crate::i18n::message("repo-core-2471", &[("i", format!("{}", i + 1).into()), ("line", (line).into())]),
                pick.is_some_and(|p| p.contains(i)),
            );
            if resp.drag_started() {
                // A drag picks the lines it goes over, from where it started.
                dragging = true;
                pick = Some(LinePick {
                    anchor: i,
                    cursor: i,
                });
            } else if resp.clicked() {
                pick = LinePick::clicked(pick, i, extend);
            } else if resp.secondary_clicked() && !pick.is_some_and(|p| p.contains(i)) {
                // The menu is for the line right-clicked, unless it is among those picked.
                pick = Some(LinePick {
                    anchor: i,
                    cursor: i,
                });
            }
            if dragging
                && let Some(p) = &mut pick
                && pointer_y.is_some_and(|y| rect.y_range().contains(y))
            {
                p.cursor = i;
            }
            let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            if location.is_some_and(|(_, line)| line == Some(i + 1))
                || pick.is_some_and(|p| p.contains(i))
            {
                p.rect_filled(rect, 0.0, t.row_selected);
            }
            if let Some(picked) = pick {
                lines_menu(&resp, data, picked, history, &mut asked, &mut links);
            }
            let gutter = Rect::from_min_size(rect.min, vec2(NUM_W, LINE_H));
            p.rect_filled(gutter, 0.0, t.card.gamma_multiply(0.6));
            let mid = rect.center().y;
            p.text(
                pos2(gutter.right() - 8.0, mid),
                Align2::RIGHT_CENTER,
                (i + 1).to_string(),
                theme::mono(11.0),
                t.text_faint,
            );
            let x = gutter.right() + 10.0;
            let spans = data.spans.as_ref().and_then(|s| s.get(i));
            let galley = line_galley(ui, spans, line, &font, t.text);
            find::paint_line(&p, (x, mid, LINE_H), galley, found.marks(i), &t);
        }
    });
    find.saw(Seen {
        x: out.state.offset.x,
        y: out.state.offset.y,
        height: out.inner_rect.height(),
        text_w: out.inner_rect.width() - NUM_W - 10.0,
        line_h: LINE_H,
    });
    if crate::keymap::copy_requested(ui.ctx())
        && let Some(picked) = pick
    {
        ui.ctx().copy_text(picked_text(data, picked));
    }
    ui.data_mut(|d| {
        d.insert_temp(pick_id, pick);
        d.insert_temp(drag_id, dragging);
    });
    asked
}

/// The text of the lines `picked`, one per line.
fn picked_text(data: &FileText, picked: LinePick) -> String {
    let (a, b) = picked.range();
    data.lines
        .get(a..=b.min(data.lines.len().saturating_sub(1)))
        .map(|l| l.join("\n"))
        .unwrap_or_default()
}

/// The right-click menu of the lines `picked`: their history, or copying them.
fn lines_menu(
    resp: &egui::Response,
    data: &FileText,
    picked: LinePick,
    history: Result<(), &str>,
    asked: &mut Option<(u32, u32)>,
    links: &mut Option<LineMenu<'_>>,
) {
    crate::menu_item::context(resp)
        .style(theme::menu_style)
        .show(|ui| {
            let (start, end) = picked.numbers();
            let which = if start == end {
                crate::i18n::message("repo-core-2472", &[("start", format!("{}", start).into())])
            } else {
                crate::i18n::message("repo-core-2473", &[("start", format!("{}", start).into()), ("end", format!("{}", end).into())])
            };
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-1133"))
                .icon("copy")
                .keys(crate::keymap::Command::SelectionCopy)
                .show(ui)
                .clicked()
            {
                ui.ctx().copy_text(picked_text(data, picked));
            }
            if let (Some(menu), Some(lines)) = (links.as_deref_mut(), Lines::new(start, end)) {
                menu(ui, lines);
            }
            match history {
                Ok(()) => {
                    let b = crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2327"))
                        .icon("history")
                        .keys(crate::keymap::Command::SelectionHistory)
                        .show(ui)
                        .on_hover_text(crate::i18n::message("repo-core-2474", &[("which", format!("{}", which).into())]));
                    if b.clicked() {
                        *asked = Some((start, end));
                    }
                }
                Err(why) => {
                    crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2327"))
                        .icon("history")
                        .keys(crate::keymap::Command::SelectionHistory)
                        .refused(Some(why))
                        .show(ui)
                        .on_disabled_hover_text(why);
                }
            }
        });
}

/// A line laid out in its syntax colours (`spans`), or all in `color` when there are none.
pub(crate) fn line_galley(
    ui: &Ui,
    spans: Option<&Spans>,
    line: &str,
    font: &egui::FontId,
    color: egui::Color32,
) -> std::sync::Arc<egui::Galley> {
    match spans {
        Some(spans) => {
            let mut job = egui::text::LayoutJob::default();
            for (color, text) in spans {
                job.append(
                    text,
                    0.0,
                    egui::TextFormat {
                        font_id: font.clone(),
                        color: *color,
                        ..Default::default()
                    },
                );
            }
            ui.painter().layout_job(job)
        }
        None => ui
            .painter()
            .layout_no_wrap(line.to_owned(), font.clone(), color),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_registry_runs_actual_file_view_and_preserves_text_ownership() {
        use crate::keymap::{self, Command, Scope, Stroke};
        use egui::{Event, Key, Modifiers};
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut settings = crate::settings::Settings::default();
        for (command, key) in [
            (Command::SelectionAll, Key::F8),
            (Command::SelectionCopy, Key::F9),
            (Command::SelectionHistory, Key::F10),
        ] {
            settings
                .key_bindings
                .insert(command.id().into(), vec![Stroke::new(key, Modifiers::NONE)]);
        }
        settings.put_for_test(&ctx);
        let data = Ok(FileText::new(
            "one\n日本語\nthree\n".as_bytes(),
            "test.txt",
            true,
        ));
        let mut find = Find::default();
        let source_calls = std::cell::Cell::new(0);
        let mut frame = |key: Option<Key>, text_owned, history| {
            let mut asked = None;
            let out = ctx.run_ui(
                egui::RawInput {
                    events: key
                        .into_iter()
                        .flat_map(|key| {
                            [true, false].map(|pressed| Event::Key {
                                key,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            })
                        })
                        .collect(),
                    ..Default::default()
                },
                |ui| {
                    keymap::active(
                        &ctx,
                        vec![Scope::Window, Scope::Repository, Scope::Selection],
                    );
                    if text_owned {
                        ui.text_edit_singleline(&mut String::new()).request_focus();
                    }
                    asked = show(
                        ui,
                        "registry-file",
                        Some(&data),
                        &mut find,
                        || {
                            source_calls.set(source_calls.get() + 1);
                            Source::DiffFile("recorded".into())
                        },
                        history,
                        None,
                        None,
                    );
                },
            );
            let copied: Vec<_> = out
                .platform_output
                .commands
                .iter()
                .filter_map(|c| match c {
                    egui::OutputCommand::CopyText(text) => Some(text.clone()),
                    _ => None,
                })
                .collect();
            out.drop_without_applying_deltas();
            (asked, copied)
        };
        frame(None, false, Ok(()));
        frame(Some(Key::F8), false, Ok(()));
        assert_eq!(
            frame(Some(Key::F9), false, Ok(())).1,
            ["one\n日本語\nthree"]
        );
        assert_eq!(frame(Some(Key::F10), false, Ok(())).0, Some((1, 3)));
        assert_eq!(
            frame(Some(Key::F10), false, Err("No committed version")).0,
            None
        );
        assert!(frame(Some(Key::F9), true, Ok(())).1.is_empty());
        assert_eq!(source_calls.get(), 0);
    }

    #[test]
    fn lines_are_picked_by_click_and_shift_click() {
        let one = LinePick::clicked(None, 4, false);
        assert_eq!(one.map(LinePick::numbers), Some((5, 5)));
        // Shift+click goes from where the pick started, either way.
        let up = LinePick::clicked(one, 1, true);
        assert_eq!(up.map(LinePick::numbers), Some((2, 5)));
        assert!(up.is_some_and(|p| p.contains(3) && !p.contains(5)));
        // A click elsewhere starts again; on the one line picked, unpicks it.
        assert_eq!(
            LinePick::clicked(up, 9, false).map(LinePick::numbers),
            Some((10, 10))
        );
        assert_eq!(LinePick::clicked(one, 4, false), None);
        // Shift+click with nothing picked picks that line.
        assert_eq!(
            LinePick::clicked(None, 2, true).map(LinePick::numbers),
            Some((3, 3))
        );
    }

    #[test]
    fn lines_lose_their_endings_and_tabs() {
        assert_eq!(split_lines("a\r\n\tb\nc"), ["a", "    b", "c"]);
        assert_eq!(split_lines("a\n\n"), ["a", ""]);
        assert!(split_lines("").is_empty());
    }

    #[test]
    fn a_nul_makes_a_file_binary() {
        let text = FileText::new(b"PNG\0\x01", "a.png", true);
        assert_eq!(text.binary, Some(5));
        assert!(text.lines.is_empty());
        let text = FileText::new(b"fn main() {}\n", "main.rs", true);
        assert_eq!((text.binary, text.lines.len(), text.longest), (None, 1, 12));
        assert!(text.spans.is_some());
    }

    #[test]
    fn a_diff_shows_the_file_after_the_change_or_before_its_deletion() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-file-text-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let git = |args: &[&str]| git::run(Some(&dir), args);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            git(args)?;
        }
        std::fs::write(dir.join("a.txt"), "one\n").map_err(|e| e.to_string())?;
        git(&["add", "-A"])?;
        git(&["commit", "-qm", "one"])?;
        std::fs::write(dir.join("a.txt"), "two\n").map_err(|e| e.to_string())?;
        git(&["commit", "-qam", "two"])?;
        git(&["rm", "-q", "a.txt"])?;
        git(&["commit", "-qm", "gone"])?;
        let commit = |rev: &str| -> Result<DiffKey, String> {
            Ok(DiffKey::Commit {
                hash: git(&["rev-parse", rev])?.trim().to_owned(),
                path: "a.txt".to_owned(),
            })
        };
        let changed = load_for_diff(&dir, &commit("HEAD~1")?, true)?;
        let deleted = load_for_diff(&dir, &commit("HEAD")?, true)?;
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            (changed.lines, changed.before),
            (vec!["two".to_owned()], false)
        );
        assert_eq!(
            (deleted.lines, deleted.before),
            (vec!["two".to_owned()], true)
        );
        Ok(())
    }
}

#[cfg(test)]
mod r2_navigation_tests {
    use super::*;
    #[test]
    fn r2_pure_pinned_file_reveal_waits_for_text_and_scrolls_to_logical_line() {
        let ctx = egui::Context::default();
        let goto = std::cell::Cell::new(Some(90));
        let mut find = Find::default();
        let mut frame = |text: Option<&Result<FileText, String>>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 300.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    show(
                        ui,
                        "pinned",
                        text,
                        &mut find,
                        || Source::File {
                            path: "unchanged.rs".into(),
                            rev: Some("pinned".into()),
                        },
                        Ok(()),
                        None,
                        Some((&goto, Some(90))),
                    );
                },
            );
            output.textures_delta.clear();
            output
        };
        let _ = frame(None);
        assert_eq!(goto.get(), Some(90));
        let text = Ok(FileText::new(
            (1..=100)
                .map(|n| format!("row{n}\n"))
                .collect::<String>()
                .as_bytes(),
            "unchanged.rs",
            false,
        ));
        let _ = frame(Some(&text));
        let output = frame(Some(&text));
        assert_eq!(goto.get(), None);
        assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::epaint::Shape::Text(text) if text.galley.text().contains("row90"))));
    }
}
