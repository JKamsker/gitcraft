//! A whole file as it is at a commit, in the index or on disk: read, coloured and shown with
//! its line numbers. For the file view's File tab, and diffs switched to the file.

use std::path::Path;

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};

use super::DiffKey;
use super::find::{self, Find, Layout, Seen, Source, SourceKey};
use super::highlight::{self, Spans};
use crate::forge::links::Lines;
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

/// What a right-click on the lines picked adds to their menu, given the lines.
pub(crate) type LineMenu<'a> = &'a mut dyn FnMut(&mut Ui, Lines);

/// The file, or a spinner, the error or a note in its place. `id`: what keeps its scrolling;
/// `find`: the find bar's search, of `source` (what shows the file).
/// `links`: the entries of the lines' right-click menu for the lines picked (a click picks one,
/// Shift+click a range); without it, lines can't be picked.
pub(crate) fn show(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: Option<&Result<FileText, String>>,
    find: &mut Find,
    source: impl FnOnce() -> Source,
    mut links: Option<LineMenu<'_>>,
) {
    let picked_id = egui::Id::new(("file_text_picked", &id));
    let t = Tokens::get(ui.ctx());
    let data = match text {
        None => {
            ui.centered_and_justified(|ui| ui.add(crate::spinner::Spinner::new()));
            return;
        }
        Some(Err(e)) => {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.colored_label(t.danger, e);
            });
            return;
        }
        Some(Ok(data)) => data,
    };
    let note = match data.binary {
        Some(size) => Some(format!("Binary file, {size} bytes")),
        None if data.lines.is_empty() => Some("Empty file".to_owned()),
        None => None,
    };
    if let Some(note) = note {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(note).color(t.text_dim));
        });
        return;
    }
    let font = theme::mono(12.0);
    let char_w = ui
        .painter()
        .layout_no_wrap("M".to_owned(), font.clone(), t.text)
        .size()
        .x;
    let width = (NUM_W + 16.0 + data.longest as f32 * char_w + 24.0).max(ui.available_width());
    let lines = &data.lines;
    if find.open {
        let key = SourceKey {
            what: source(),
            at: lines.as_ptr() as usize,
            len: lines.len(),
        };
        let text = |i| find::nth(lines, i);
        find.update(ui.ctx(), key, lines.len(), text, Layout::Lines);
    }

    ui.spacing_mut().item_spacing.y = 0.0;
    let sense = if links.is_some() {
        Sense::click()
    } else {
        Sense::hover()
    };
    let mut picked = ui
        .data(|d| d.get_temp::<Picked>(picked_id))
        .filter(|p| p.last() < data.lines.len());
    let shift = ui.input(|i| i.modifiers.shift);
    let mut scroll = egui::ScrollArea::both()
        .id_salt(("file_text", id))
        .auto_shrink(false);
    if let Some(hit) = find.take_reveal() {
        let text = find::nth(lines, hit.line).unwrap_or_default();
        scroll = find.scroll_to(scroll, &hit, text, char_w);
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    let find_ref = &*find;
    let out = scroll.show_rows(ui, LINE_H, lines.len(), |ui, range| {
            for i in range {
            let Some(line) = lines.get(i) else {
                    continue;
                };
                let (rect, resp) = ui.allocate_exact_size(vec2(width, LINE_H), sense);
                if let Some(links) = links.as_deref_mut() {
                    picked = pick_line(picked, i, &resp, shift);
                    if let Some(lines) = picked.and_then(|p| p.lines()) {
                        egui::Popup::context_menu(&resp)
                            .style(theme::menu_style)
                            .show(|ui| links(ui, lines));
                    }
                }
                let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                if picked.is_some_and(|p| p.contains(i)) {
                    p.rect_filled(rect, 0.0, t.row_selected);
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
            let spans = data.spans.as_ref().and_then(|s| s.get(i));
            let galley = line_galley(ui, spans, line, &font, t.text);
            find::paint_line(
                &p,
                (gutter.right() + 10.0, mid, LINE_H),
                galley,
                find_ref.marks(i),
                &t,
            );
        }
    });
    find.saw(Seen {
        x: out.state.offset.x,
        y: out.state.offset.y,
        height: out.inner_rect.height(),
        text_w: out.inner_rect.width() - NUM_W - 10.0,
        line_h: LINE_H,
    });
    if links.is_some() {
        ui.data_mut(|d| match picked {
            Some(p) => {
                d.insert_temp(picked_id, p);
            }
            None => d.remove::<Picked>(picked_id),
        });
    }
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

/// The lines picked in a file shown: from the one clicked first to the one Shift+clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Picked {
    anchor: usize,
    cursor: usize,
}

impl Picked {
    fn contains(self, i: usize) -> bool {
        (self.anchor.min(self.cursor)..=self.last()).contains(&i)
    }

    fn last(self) -> usize {
        self.anchor.max(self.cursor)
    }

    /// The lines as the file numbers them, from 1.
    fn lines(self) -> Option<Lines> {
        let number = |i: usize| u32::try_from(i + 1).ok();
        Lines::new(number(self.anchor)?, number(self.cursor)?)
    }
}

/// The lines picked after a click on line `i` (`resp`): a click picks it (again: unpicks it),
/// Shift+click picks up to it, a right-click outside those picked picks it for the menu.
fn pick_line(
    picked: Option<Picked>,
    i: usize,
    resp: &egui::Response,
    shift: bool,
) -> Option<Picked> {
    let one = Some(Picked {
        anchor: i,
        cursor: i,
    });
    if resp.clicked() {
        return match picked {
            Some(p) if shift => Some(Picked { cursor: i, ..p }),
            Some(p) if p.anchor == i && p.cursor == i => None,
            _ => one,
        };
    }
    if resp.secondary_clicked() && !picked.is_some_and(|p| p.contains(i)) {
        return one;
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clicking line 2, Shift+clicking line 4 and right-clicking line 3 opens the lines' menu
    /// for lines 2–4; right-clicking a line outside them then opens it for that line alone.
    #[test]
    fn lines_picked_are_what_the_menu_links_to() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let text: Result<FileText, String> = Ok(FileText::new(
            b"one\ntwo\nthree\nfour\nfive\n",
            "f.txt",
            true,
        ));
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
        let row = |n: f32| pos2(200.0, LINE_H * (n - 0.5));
        let mut asked: Vec<Lines> = Vec::new();
        let frame = |mut events: Vec<egui::Event>, shift: bool, asked: &mut Vec<Lines>| {
            let modifiers = if shift {
                egui::Modifiers::SHIFT
            } else {
                egui::Modifiers::NONE
            };
            events.insert(0, egui::Event::ModifiersChanged(modifiers));
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        let mut menu = |_: &mut Ui, lines: Lines| asked.push(lines);
                        show(ui, "test", Some(&text), Some(&mut menu));
                    });
            })
            .drop_without_applying_deltas();
        };
        let click = |at, button, shift: bool| {
            let modifiers = if shift {
                egui::Modifiers::SHIFT
            } else {
                egui::Modifiers::NONE
            };
            [true, false]
                .map(|pressed| egui::Event::PointerButton {
                    pos: at,
                    button,
                    pressed,
                    modifiers,
                })
                .to_vec()
        };
        let primary = egui::PointerButton::Primary;
        let secondary = egui::PointerButton::Secondary;
        for (at, button, shift) in [
            (row(2.0), primary, false),
            (row(4.0), primary, true),
            (row(3.0), secondary, false),
        ] {
            frame(vec![egui::Event::PointerMoved(at)], shift, &mut asked);
            frame(click(at, button, shift), shift, &mut asked);
        }
        frame(Vec::new(), false, &mut asked);
        assert_eq!(asked.last(), Lines::new(2, 4).as_ref(), "{asked:?}");

        // Away from the menu, a right-click on line 5: that line alone.
        frame(vec![egui::Event::PointerMoved(row(5.0))], false, &mut asked);
        frame(click(row(5.0), secondary, false), false, &mut asked);
        frame(Vec::new(), false, &mut asked);
        assert_eq!(asked.last(), Lines::new(5, 5).as_ref(), "{asked:?}");
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
