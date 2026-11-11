//! Names as a tree: `docs/accuracy/gearbox` is a branch in a folder in a folder, and
//! `src/repo/mod.rs` a changed file in one.
//!
//! Branch folders start collapsed, file folders open (those of all the files of a commit
//! collapsed). The ones changed are saved with the app.

use std::collections::HashMap;

use egui::{Align2, Id, Rect, Response, Sense, Ui, pos2, vec2};

use super::sidebar::ROW_H;
use super::widgets;
use crate::a11y::labels;
use crate::icons;
use crate::settings::Settings;
use crate::theme::{self, Tokens};

/// How much further in each level of folders is.
const INDENT: f32 = 14.0;

#[derive(Debug, PartialEq)]
pub(super) enum Node<'a, T> {
    Folder(Folder<'a, T>),
    Leaf { name: &'a str, item: T },
}

#[derive(Debug, PartialEq)]
pub(super) struct Folder<'a, T> {
    pub name: &'a str,
    /// The folder's whole path, which is what identifies it: `docs/accuracy`.
    pub path: &'a str,
    pub children: Vec<Node<'a, T>>,
}

// Folders can nest as deep as a repository makes them (thousands of levels, in one made to
// do it): nothing here recurses over them, which would run out of stack (1 MiB for the main
// thread on Windows).

impl<T> Drop for Folder<'_, T> {
    /// A level at a time: dropped recursively, deep folders would run out of stack.
    fn drop(&mut self) {
        let mut rest = std::mem::take(&mut self.children);
        while let Some(node) = rest.pop() {
            if let Node::Folder(mut folder) = node {
                rest.append(&mut folder.children);
            }
        }
    }
}

impl<T> Folder<'_, T> {
    /// How many branches are in here, at any depth.
    pub fn leaves(&self) -> usize {
        let mut count = 0;
        let mut levels = vec![self.children.as_slice()];
        while let Some(nodes) = levels.pop() {
            for node in nodes {
                match node {
                    Node::Folder(folder) => levels.push(&folder.children),
                    Node::Leaf { .. } => count += 1,
                }
            }
        }
        count
    }

    /// Whether a branch in here, at any depth, satisfies `pred`.
    pub fn any(&self, pred: &impl Fn(&T) -> bool) -> bool {
        let mut levels = vec![self.children.as_slice()];
        while let Some(nodes) = levels.pop() {
            for node in nodes {
                match node {
                    Node::Folder(folder) => levels.push(&folder.children),
                    Node::Leaf { item, .. } if pred(item) => return true,
                    Node::Leaf { .. } => {}
                }
            }
        }
        false
    }
}

/// What goes in a folder while the tree is built.
enum Part<'a, T> {
    /// A folder, by where it is in the list of them.
    Folder(usize),
    Leaf(&'a str, T),
}

/// The tree of `(name, item)`. Siblings keep the order they first appear in, so a sorted list
/// stays sorted.
pub(super) fn build<'a, T>(items: impl IntoIterator<Item = (&'a str, T)>) -> Vec<Node<'a, T>> {
    // The folders in the order they are found, each after the one holding it; the first holds
    // the top level.
    let mut found: Vec<(&'a str, &'a str, Vec<Part<'a, T>>)> = vec![("", "", Vec::new())];
    // Each folder by the one holding it and its own name: a folder can hold thousands of files,
    // and looking through them for each would be quadratic; by its whole path, hashing each
    // folder of each name would take time growing with the cube of how deep they go.
    let mut folders: HashMap<(usize, &'a str), usize> = HashMap::new();
    // The name before, and its folders: where each one's name ends in it, and which it is.
    // Those the next name shares (most of them, in a sorted list) need no looking up.
    let mut before = "";
    let mut opened: Vec<(usize, usize)> = Vec::new();
    for (full, item) in items {
        let shared = opened.partition_point(|&(end, _)| {
            end + 1 < full.len() && full.as_bytes().get(..=end) == before.as_bytes().get(..=end)
        });
        opened.truncate(shared);
        let (mut parent, mut start) = opened.last().map_or((0, 0), |&(end, f)| (f, end + 1));
        // A slash at the end doesn't open a folder: git shows a nested repository as `sub/`.
        while let Some(end) = full[start..]
            .find('/')
            .map(|i| start + i)
            .filter(|&end| end + 1 < full.len())
        {
            let name = &full[start..end];
            let next = found.len();
            let folder = *folders.entry((parent, name)).or_insert(next);
            if folder == next {
                found.push((name, &full[..end], Vec::new()));
                if let Some((_, _, parts)) = found.get_mut(parent) {
                    parts.push(Part::Folder(folder));
                }
            }
            opened.push((end, folder));
            parent = folder;
            start = end + 1;
        }
        before = full;
        if let Some((_, _, parts)) = found.get_mut(parent) {
            parts.push(Part::Leaf(&full[start..], item));
        }
    }
    // From the last folder found back to the first: those in a folder are made before it.
    let mut made: Vec<Option<Folder<'a, T>>> = (0..found.len()).map(|_| None).collect();
    while let Some((name, path, parts)) = found.pop() {
        let children = parts
            .into_iter()
            .filter_map(|part| match part {
                Part::Folder(i) => made.get_mut(i).and_then(Option::take).map(Node::Folder),
                Part::Leaf(name, item) => Some(Node::Leaf { name, item }),
            })
            .collect();
        if let Some(slot) = made.get_mut(found.len()) {
            *slot = Some(Folder {
                name,
                path,
                children,
            });
        }
    }
    match made.first_mut().and_then(Option::take) {
        Some(mut top) => std::mem::take(&mut top.children),
        None => Vec::new(),
    }
}

/// What a branch's row needs from the tree.
#[derive(Clone, Copy)]
pub(super) struct LeafRow<'a> {
    /// The last part of the branch's name: its folders are shown above it.
    pub name: &'a str,
    pub indent: f32,
}

/// Merges each folder whose only child is a folder with it, `src/repo` instead of `src` holding
/// `repo`, and puts folders before what isn't one, at every level.
pub(super) fn compact<T>(nodes: &mut [Node<'_, T>]) {
    let mut levels = vec![nodes];
    while let Some(nodes) = levels.pop() {
        nodes.sort_by_key(|node| !matches!(node, Node::Folder(_)));
        for node in nodes {
            if let Node::Folder(folder) = node {
                while let [Node::Folder(_)] = folder.children.as_slice() {
                    let Some(Node::Folder(mut child)) = folder.children.pop() else {
                        break;
                    };
                    let start = folder.path.len() - folder.name.len();
                    folder.name = &child.path[start..];
                    folder.path = child.path;
                    folder.children = std::mem::take(&mut child.children);
                }
                levels.push(&mut folder.children);
            }
        }
    }
}

/// How to show a tree.
#[derive(Clone, Copy)]
pub(super) struct TreeView {
    /// Identifies the tree among those of all repositories.
    pub id: Id,
    /// While a filter is on, every folder with a match is open and stays as it was saved.
    pub filtering: bool,
    /// Where the tree starts.
    pub indent: f32,
    /// Whether a folder nobody has opened or closed yet is open.
    pub start_open: bool,
    /// Where a top-level folder's chevron is, from the left of the row.
    pub left: f32,
    /// How tall a leaf's row is, when every one is as tall: rows out of sight are then only
    /// skipped over, not drawn.
    pub leaf_h: Option<f32>,
}

impl TreeView {
    /// Branches in the sidebar: folders stay closed until opened.
    pub fn branches(id: Id, filtering: bool, indent: f32) -> Self {
        Self {
            id,
            filtering,
            indent,
            start_open: false,
            left: 34.0,
            // A filter opens every folder with a match: thousands of rows.
            leaf_h: Some(ROW_H),
        }
    }
}

/// Files, flat or as a tree of their folders as the settings say. `row` draws a file, given how
/// far in it is in the tree (`None`: in a flat list, showing its folder); a folder holding the
/// file for which `selected` holds stands out while it is closed. `start_open`: whether a
/// folder nobody has opened or closed yet is open.
pub(super) fn files<'a, T>(
    ui: &mut Ui,
    (id, start_open): (Id, bool),
    files: &'a [T],
    path: impl Fn(&'a T) -> &'a str,
    selected: &impl Fn(&&'a T) -> bool,
    row: &mut impl FnMut(&mut Ui, Option<f32>, &'a T),
) -> Option<Rect> {
    let mut reveal_rect = None;
    let reveal_id = id.with("keyboard-reveal");
    let reveal = ui.data_mut(|d| {
        let path = d.get_temp::<String>(reveal_id);
        d.remove::<String>(reveal_id);
        path
    });
    if !Settings::get(ui.ctx()).files_as_tree {
        if let Some(target) = &reveal
            && let Some(i) = files.iter().position(|file| path(file) == target)
        {
            let pitch = widgets::ROW_H + ui.spacing().item_spacing.y;
            let rect = Rect::from_min_size(
                ui.cursor().min + vec2(0.0, i as f32 * pitch),
                vec2(ui.available_width(), widgets::ROW_H),
            );
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
            reveal_rect = Some(rect);
        }
        // A commit can change tens of thousands of files: only the rows in sight are drawn.
        // While a menu is open (perhaps one of a row out of sight, which must go on being
        // shown), row by row.
        if egui::Popup::is_any_open(ui.ctx()) {
            for f in files {
                if widgets::keyed_row_menu_open(ui, file_key(path(f)))
                    || !skip_unseen(ui, widgets::ROW_H)
                {
                    row(ui, None, f);
                }
            }
            return reveal_rect;
        }
        let pitch = widgets::ROW_H + ui.spacing().item_spacing.y;
        let top = ui.cursor().top();
        let clip = ui.clip_rect();
        let first = (((clip.top() - top) / pitch).floor().max(0.0) as usize).min(files.len());
        let end = ((((clip.bottom() - top) / pitch).ceil().max(0.0) as usize) + 1)
            .clamp(first, files.len());
        if first > 0 {
            ui.add_space(first as f32 * pitch);
            // The rows in sight keep the ids they would have with every row drawn.
            ui.skip_ahead_auto_ids(first);
        }
        for f in &files[first..end] {
            row(ui, None, f);
        }
        if end < files.len() {
            ui.add_space((files.len() - end) as f32 * pitch);
            // What follows the list (another list) keeps its ids too.
            ui.skip_ahead_auto_ids(files.len() - end);
        }
        return reveal_rect;
    }
    let mut tree = build(files.iter().map(|f| (path(f), f)));
    compact(&mut tree);
    let view = TreeView {
        id,
        filtering: false,
        indent: 0.0,
        start_open,
        left: 18.0,
        leaf_h: Some(widgets::ROW_H),
    };
    if let Some(target) = &reveal {
        let mut y = ui.cursor().top();
        let mut levels = vec![tree.iter()];
        while let Some(level) = levels.last_mut() {
            match level.next() {
                Some(Node::Leaf { item, .. }) => {
                    if path(item) == target {
                        let rect = Rect::from_min_size(
                            pos2(ui.cursor().left(), y),
                            vec2(ui.available_width(), widgets::ROW_H),
                        );
                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                        reveal_rect = Some(rect);
                        break;
                    }
                    y += widgets::ROW_H + ui.spacing().item_spacing.y;
                }
                Some(Node::Folder(folder)) => {
                    y += ROW_H + ui.spacing().item_spacing.y;
                    let saved = ui
                        .data_mut(|d| d.get_persisted::<bool>(id.with(folder.path)))
                        .unwrap_or(false);
                    if saved != start_open {
                        levels.push(folder.children.iter());
                    }
                }
                None => {
                    levels.pop();
                }
            }
        }
    }
    let key = |f: &&'a T| file_key(path(f));
    show(ui, &tree, &view, selected, &key, &mut |ui, leaf, f| {
        row(ui, Some(leaf.indent), f);
    });
    reveal_rect
}

/// What [`widgets::file_row`] knows the row of the file at `path` by.
fn file_key(path: &str) -> (&'static str, &str) {
    ("file-row", path)
}

/// The panel-header switch that shows files as a flat list of their paths or as a tree of their
/// folders.
pub(super) fn files_toggle(ui: &mut Ui) {
    let mut settings = Settings::get(ui.ctx());
    let on = usize::from(settings.files_as_tree);
    if let Some(picked) = widgets::switch(
        ui,
        &[
            crate::i18n::text("repo-core-0250"),
            crate::i18n::text("repo-core-0251"),
        ],
        on,
    ) {
        settings.files_as_tree = picked == 1;
        settings.set(ui.ctx());
    }
}

/// Shows the rows of a tree, with `leaf` drawing the branches. A folder with a branch for which
/// `is_head` holds stands out, so that a collapsed tree still shows where HEAD is. `key` is what
/// `leaf` gives its row to [`widgets::keyed_row`]: a row out of sight is drawn all the same
/// while its menu is open, as a menu not shown for a frame closes.
pub(super) fn show<'a, T, K: egui::AsIdSalt>(
    ui: &mut Ui,
    nodes: &[Node<'a, T>],
    view: &TreeView,
    is_head: &impl Fn(&T) -> bool,
    key: &impl Fn(&T) -> K,
    leaf: &mut impl FnMut(&mut Ui, LeafRow<'a>, &T),
) {
    // The folders open, outermost first, each with what is left to show of it.
    let mut levels = vec![(nodes.iter(), view.indent)];
    while let Some((nodes, indent)) = levels.last_mut() {
        let indent = *indent;
        let Some(node) = nodes.next() else {
            levels.pop();
            continue;
        };
        let view = &TreeView { indent, ..*view };
        match node {
            Node::Leaf { item, .. }
                if view.leaf_h.is_some_and(|h| {
                    let menu = egui::Popup::is_any_open(ui.ctx())
                        && widgets::keyed_row_menu_open(ui, key(item));
                    !menu && skip_unseen(ui, h)
                }) => {}
            Node::Leaf { name, item } => {
                let row = LeafRow { name, indent };
                leaf(ui, row, item);
            }
            Node::Folder(folder) => {
                let id = view.id.with(folder.path);
                // Only the folders that differ from how they start are kept.
                let saved = ui
                    .data_mut(|d| d.get_persisted::<bool>(id))
                    .unwrap_or(false);
                let open = view.filtering || saved != view.start_open;
                let toggled = !skip_unseen(ui, ROW_H)
                    && folder_row(ui, folder, view, open, folder.any(is_head)).clicked()
                    && !view.filtering;
                if toggled {
                    ui.data_mut(|d| {
                        if saved {
                            d.remove::<bool>(id);
                        } else {
                            d.insert_persisted(id, true);
                        }
                    });
                }
                if open != toggled {
                    levels.push((folder.children.iter(), indent + INDENT));
                }
            }
        }
    }
}

/// Moves past a row `h` tall instead of drawing it, when it would be out of sight. The rows
/// after it keep the ids they would have had. A row whose menu is open is drawn anyway: a
/// menu not shown for a frame closes.
pub(super) fn skip_unseen(ui: &mut Ui, h: f32) -> bool {
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), h));
    // The row's response would get the next id, and its menu that id's (see
    // `egui::Popup::default_response_id`).
    let menu = ui.next_auto_id().with("popup");
    if ui.is_rect_visible(rect) || egui::Popup::is_id_open(ui.ctx(), menu) {
        return false;
    }
    ui.add_space(h + ui.spacing().item_spacing.y);
    ui.skip_ahead_auto_ids(1);
    true
}

/// A folder: chevron where its branches have their icon, name, how many branches it holds.
fn folder_row<T>(
    ui: &mut Ui,
    folder: &Folder<'_, T>,
    view: &TreeView,
    open: bool,
    has_head: bool,
) -> Response {
    let t = Tokens::get(ui.ctx());
    let x = view.left + view.indent;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    crate::a11y::a11y(&resp, egui::WidgetType::CollapsingHeader, || {
        let count = labels::count(folder.leaves(), "item", "items");
        labels::section(
            &crate::i18n::message(
                "repo-core-0255",
                &[("name", folder.name.to_string().into())],
            ),
            &count,
            open,
        )
    });
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, t.hover);
    }
    let y = rect.center().y;
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + x, y), vec2(14.0, 14.0)),
        if open {
            "chevron-down"
        } else {
            "chevron-right"
        },
        12.0,
        if has_head { t.accent } else { t.text_faint },
    );
    let count = painter.text(
        pos2(rect.right() - 10.0, y),
        Align2::RIGHT_CENTER,
        folder.leaves().to_string(),
        theme::medium(11.0),
        t.text_faint,
    );
    let font = if has_head {
        theme::semibold(12.5)
    } else {
        egui::FontId::proportional(12.5)
    };
    painter
        .with_clip_rect(rect.with_max_x(count.left() - 6.0))
        .text(
            pos2(rect.left() + x + 14.0, y),
            Align2::LEFT_CENTER,
            folder.name,
            font,
            t.text,
        );
    resp
}

#[cfg(test)]
#[path = "direct_tests/branch_tree_tests.rs"]
mod tests;
