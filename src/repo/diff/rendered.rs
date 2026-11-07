//! A Markdown file of a diff rendered: as it is after the change, or its changes, block by
//! block, the removed ones in red and the added ones in green. Images of the repository show as
//! they are on each side of the change; any other image is a link, so that reading a file never
//! makes the app contact a server its author chose.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use egui::{Stroke, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::git::{self, Area};
use crate::pull_request as pr;
use crate::repo::DiffKey;
use crate::theme::Tokens;

const MD_EXTS: &[&str] = &["md", "markdown", "mdown", "mkd", "mkdn", "mdwn"];
/// The page's text wraps to this at most, as the pull request's does.
const CONTENT_W: f32 = 900.0;
const MARGIN_X: f32 = 24.0;
/// Images of the repository shown at most, per side, and the most bytes each may have.
const MAX_IMAGES: usize = 64;
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
/// Past this many pairs of blocks to compare, the changed middle is shown removed then added.
const MAX_DIFF_CELLS: usize = 4_000_000;

/// Whether `path` is a Markdown file, which diffs can show rendered.
pub fn is_markdown(path: &str) -> bool {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| MD_EXTS.contains(&e.as_str()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Kept,
    Removed,
    Added,
}

/// What the rendered views show, prepared on a background thread.
pub struct RenderedMd {
    /// The file as it is after the change (before it, when the change deleted it), made safe.
    pub file: String,
    /// The change deleted the file: [`RenderedMd::file`] is from before it.
    pub before: bool,
    /// The blocks of the file before and after the change, in order, each kept, removed or added.
    pub blocks: Vec<(Change, String)>,
    /// The pictures the text shows from the repository: their address, and what they hold.
    pub images: Vec<(String, Arc<[u8]>)>,
}

/// Reads both sides of the diff `key` and prepares them to be rendered.
pub fn load(repo: &Path, key: &DiffKey) -> Result<RenderedMd, String> {
    let (old, new) = sides(key);
    let read = |side: &Option<(Option<String>, String)>| -> Option<(Side, String)> {
        let (rev, path) = side.as_ref()?;
        let bytes = git::file_at(repo, rev.as_deref(), path).ok()?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        Some((
            Side {
                rev: rev.clone(),
                path: path.clone(),
            },
            text,
        ))
    };
    let (old, new) = (read(&old), read(&new));
    if old.is_none() && new.is_none() {
        return Err("Couldn't read the file on either side of the change".to_owned());
    }
    let mut images = Images::default();
    let mut safe = |side: &Option<(Side, String)>| {
        side.as_ref()
            .map(|(side, text)| images.safe_text(repo, side, text))
    };
    let old = safe(&old);
    let new = safe(&new);
    let trusted = images.uris.clone();
    let check = |text: &str| pr::images_to_links_except(text, &|url| trusted.contains(url));
    let (old_blocks, old_defs) = old.as_deref().map(split_blocks).unwrap_or_default();
    let (new_blocks, new_defs) = new.as_deref().map(split_blocks).unwrap_or_default();
    // A block can read differently on its own, so each is checked for images again; the link
    // definitions of its side go along, for the links that use them.
    let shown = |block: &str, defs: &str| {
        if defs.is_empty() {
            check(block)
        } else {
            check(&format!("{block}\n\n{defs}"))
        }
    };
    let blocks = diff_blocks(&old_blocks, &new_blocks)
        .into_iter()
        .map(|(change, i)| {
            let text = match change {
                Change::Removed => shown(&old_blocks[i], &old_defs),
                _ => shown(&new_blocks[i], &new_defs),
            };
            (change, text)
        })
        .collect();
    let before = new.is_none();
    Ok(RenderedMd {
        file: new.or(old).unwrap_or_default(),
        before,
        blocks,
        images: images.found,
    })
}

/// A side of the change: the revision (`None`: on disk; `""`: the index) and path of the file.
struct Side {
    rev: Option<String>,
    path: String,
}

type SideAt = Option<(Option<String>, String)>;

/// Where the file is before and after the change, when it is on that side.
fn sides(key: &DiffKey) -> (SideAt, SideAt) {
    let at = |rev: Option<&str>, path: &str| Some((rev.map(str::to_owned), path.to_owned()));
    match key {
        DiffKey::Commit { hash, path } => {
            (at(Some(&format!("{hash}^")), path), at(Some(hash), path))
        }
        DiffKey::Work(file) => {
            let old_path = file.old_path.as_deref().unwrap_or(&file.path);
            match file.area {
                Area::Staged => (at(Some("HEAD"), old_path), at(Some(""), &file.path)),
                Area::Unstaged => (at(Some(""), &file.path), at(None, &file.path)),
                Area::Untracked | Area::Conflicted => (None, at(None, &file.path)),
            }
        }
        DiffKey::Range { from, to, path } => (at(Some(from), path), at(to.as_deref(), path)),
        DiffKey::Remerge { hash, path, .. } => (None, at(Some(hash), path)),
    }
}

/// The pictures of the repository the texts show, read once each.
#[derive(Default)]
struct Images {
    found: Vec<(String, Arc<[u8]>)>,
    uris: HashSet<String>,
    /// How many were looked for on the side being read.
    tried: usize,
}

impl Images {
    /// `text`, a side's file, made safe, with its images of the repository read as they are
    /// on that side.
    fn safe_text(&mut self, repo: &Path, side: &Side, text: &str) -> String {
        self.tried = 0;
        let (text, _) = pr::safe_local_markdown(text, &mut |url| self.local(repo, side, url));
        text
    }

    /// The address to show the image at `url` from, when it is a file of the repository.
    fn local(&mut self, repo: &Path, side: &Side, url: &str) -> Option<String> {
        let path = repo_path(&side.path, url)?;
        if self.tried >= MAX_IMAGES {
            return None;
        }
        self.tried += 1;
        let bytes = match side.rev {
            None => working_tree_image(repo, &path)?,
            Some(_) => git::file_at(repo, side.rev.as_deref(), &path).ok()?,
        };
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
            return None;
        }
        let uri = image_uri(&path, &bytes);
        if self.uris.insert(uri.clone()) {
            self.found
                .push((uri.clone(), Arc::from(bytes.into_boxed_slice())));
        }
        Some(uri)
    }
}

/// Named by what it holds, as the pictures of a diff are (see `images::image_id`); the name of
/// the file stays at the end, where the loaders look for its type.
fn image_uri(path: &str, bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let name: String = path
        .rsplit('/')
        .next()
        .unwrap_or("image")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("bytes://markdown/{:016x}/{name}", h.finish())
}

/// The file of the repository an image at `url` in the file `file` stands for: relative to the
/// file's folder, or to the repository's top with a leading `/`, as forges read it. Nothing for
/// an address elsewhere (with a scheme, a drive, a server…) or one leading out of the repository.
fn repo_path(file: &str, url: &str) -> Option<String> {
    let url = url.split(['#', '?']).next().unwrap_or("");
    let url = percent_decoded(url)?;
    if url.is_empty() || url.contains(['\\', ':', '\0']) || url.starts_with("//") {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    if !url.starts_with('/') {
        parts.extend(file.split('/'));
        parts.pop();
    }
    for part in url.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `text` with its `%xx` escapes decoded; nothing when that isn't UTF-8.
fn percent_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            )
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The top-level blocks of `text` (a heading, a paragraph, a code block, each item of a list…),
/// and its link definitions, which the blocks' links may need.
fn split_blocks(text: &str) -> (Vec<String>, String) {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let mut events = Parser::new_ext(text, pr::markdown_options()).into_offset_iter();
    let defs: Vec<&str> = events
        .reference_definitions()
        .iter()
        .filter_map(|(_, def)| text.get(def.span.clone()))
        .map(str::trim_end)
        .collect();
    let mut blocks = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut in_list = false;
    let push = |blocks: &mut Vec<String>, range: std::ops::Range<usize>| {
        if let Some(block) = text.get(range).map(str::trim_end)
            && !block.trim().is_empty()
        {
            blocks.push(block.to_owned());
        }
    };
    for (event, range) in events.by_ref() {
        match event {
            Event::Start(tag) => {
                depth += 1;
                if depth == 1 {
                    start = range.start;
                    in_list = matches!(tag, Tag::List(_));
                } else if depth == 2 && in_list && matches!(tag, Tag::Item) {
                    start = range.start;
                }
            }
            Event::End(tag) => {
                depth = depth.saturating_sub(1);
                let item_ends = depth == 1 && in_list && tag == TagEnd::Item;
                if item_ends || (depth == 0 && !in_list) {
                    push(&mut blocks, start..range.end);
                }
            }
            // Blocks without an end: a rule, and what is left outside of any block.
            _ if depth == 0 => push(&mut blocks, range),
            _ => {}
        }
    }
    (blocks, defs.join("\n"))
}

/// Which blocks of `old` the change removed and which of `new` it added, in reading order, with
/// the place of each in its side (in `new` for those kept).
fn diff_blocks(old: &[String], new: &[String]) -> Vec<(Change, usize)> {
    let same = |a: &String, b: &String| a.trim() == b.trim();
    let prefix = old.iter().zip(new).take_while(|(a, b)| same(a, b)).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| same(a, b))
        .count();
    let (old_mid, new_mid) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    let mut out: Vec<(Change, usize)> = (0..prefix).map(|i| (Change::Kept, i)).collect();
    let (n, m) = (old_mid.len(), new_mid.len());
    if n.saturating_mul(m) > MAX_DIFF_CELLS {
        out.extend((0..n).map(|i| (Change::Removed, prefix + i)));
        out.extend((0..m).map(|j| (Change::Added, prefix + j)));
    } else {
        // The longest run of blocks both have, from the end: `lcs[i][j]` for old_mid[i..] and
        // new_mid[j..].
        let mut lcs = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if same(&old_mid[i], &new_mid[j]) {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && same(&old_mid[i], &new_mid[j]) {
                out.push((Change::Kept, prefix + j));
                i += 1;
                j += 1;
            } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
                out.push((Change::Removed, prefix + i));
                i += 1;
            } else {
                out.push((Change::Added, prefix + j));
                j += 1;
            }
        }
    }
    let new_tail = new.len() - suffix;
    out.extend((new_tail..new.len()).map(|j| (Change::Kept, j)));
    out
}

/// Gives egui the pictures of `md`, which it keeps by their address.
pub fn include_images(ctx: &egui::Context, md: &RenderedMd) {
    for (uri, bytes) in &md.images {
        ctx.include_bytes(uri.clone(), egui::load::Bytes::Shared(bytes.clone()));
    }
}

/// The file rendered (`changes`: its changes), scrolled on its own; `id` keeps its scrolling.
pub fn show(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    md: &RenderedMd,
    cache: &mut CommonMarkCache,
    changes: bool,
) {
    let t = Tokens::get(ui.ctx());
    let width = (ui.available_width() - 2.0 * MARGIN_X).min(CONTENT_W);
    egui::ScrollArea::both()
        .id_salt(("rendered-md", id, changes))
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(MARGIN_X as i8, 18))
                .show(ui, |ui| {
                    ui.set_max_width(width);
                    if !changes {
                        markdown(ui, cache, &md.file, width);
                        return;
                    }
                    if md.blocks.iter().all(|(change, _)| *change == Change::Kept) {
                        ui.label(
                            egui::RichText::new("The change leaves the rendered text as it was.")
                                .color(t.text_dim)
                                .italics(),
                        );
                        ui.add_space(12.0);
                    }
                    for (i, (change, text)) in md.blocks.iter().enumerate() {
                        ui.push_id(i, |ui| block(ui, &t, cache, *change, text, width));
                    }
                });
        });
    crate::repo::pr_view::only_web_links(ui.ctx());
}

/// A block of the changes: as it is when kept, on red or green with a bar on its left when
/// removed or added.
fn block(
    ui: &mut Ui,
    t: &Tokens,
    cache: &mut CommonMarkCache,
    change: Change,
    text: &str,
    width: f32,
) {
    let (fill, bar) = match change {
        Change::Kept => {
            markdown(ui, cache, text, width);
            return;
        }
        Change::Removed => (t.diff_del_bg, t.danger),
        Change::Added => (t.diff_add_bg, t.success),
    };
    let inner = width - 14.0;
    let frame = egui::Frame::new()
        .fill(fill)
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin {
            left: 10,
            right: 4,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.set_width(inner);
            markdown(ui, cache, text, inner);
        });
    let rect = frame.response.rect;
    ui.painter()
        .line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(3.0, bar));
    let tip = match change {
        Change::Removed => "Removed",
        _ => "Added",
    };
    frame.response.on_hover_text(tip);
    ui.add_space(2.0);
}

fn markdown(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str, width: f32) {
    ui.set_max_width(width);
    CommonMarkViewer::new()
        .max_image_width(Some(width as usize))
        .show(ui, cache, text);
}

/// The working tree's image `path` (a path in the repository, `/` between names), when it is a
/// plain file of the repository, not reached through a link (`docs/l -> /` would read
/// anything), and at most an image's limit. Checked on the file that is read, so that nothing
/// swapped in meanwhile is read instead (see [`crate::safe_read`]); on another machine, checked
/// and read there in one go.
fn working_tree_image(repo: &Path, path: &str) -> Option<Vec<u8>> {
    if let Some(read) = crate::remote::read_within(repo, path, MAX_IMAGE_BYTES as u64) {
        return read.ok().flatten();
    }
    crate::safe_read::read_inside(repo, path, MAX_IMAGE_BYTES as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn markdown_files_are_known_by_their_extension() {
        assert!(is_markdown("README.md"));
        assert!(is_markdown("docs/Guide.MarkDown"));
        assert!(!is_markdown("src/main.rs"));
        assert!(!is_markdown("md"));
    }

    #[test]
    fn blocks_are_the_top_level_ones_and_each_item_of_a_list() {
        let text = "# Title\n\nSome text\non two lines.\n\n- one\n- two\n\n---\n\n```rust\nfn x() {}\n```\n\n> quoted\n> - not split\n";
        let (blocks, defs) = split_blocks(text);
        assert_eq!(
            blocks,
            strings(&[
                "# Title",
                "Some text\non two lines.",
                "- one",
                "- two",
                "---",
                "```rust\nfn x() {}\n```",
                "> quoted\n> - not split",
            ])
        );
        assert!(defs.is_empty());
    }

    #[test]
    fn link_definitions_go_along_with_the_blocks() {
        let (blocks, defs) = split_blocks("See [the docs][d].\n\n[d]: https://example.org\n");
        assert_eq!(blocks, strings(&["See [the docs][d]."]));
        assert_eq!(defs, "[d]: https://example.org");
    }

    #[test]
    fn the_changed_blocks_are_removed_then_added_between_those_kept() {
        let old = strings(&["a", "b", "c", "d"]);
        let new = strings(&["a", "B", "c", "d", "e"]);
        assert_eq!(
            diff_blocks(&old, &new),
            vec![
                (Change::Kept, 0),
                (Change::Removed, 1),
                (Change::Added, 1),
                (Change::Kept, 2),
                (Change::Kept, 3),
                (Change::Added, 4),
            ]
        );
        // A new file, a deleted one.
        assert_eq!(diff_blocks(&[], &strings(&["x"])), vec![(Change::Added, 0)]);
        assert_eq!(
            diff_blocks(&strings(&["x"]), &[]),
            vec![(Change::Removed, 0)]
        );
        // A block moved: kept where the longest run of the same ones is.
        assert_eq!(
            diff_blocks(&strings(&["a", "b", "c"]), &strings(&["b", "c", "a"])),
            vec![
                (Change::Removed, 0),
                (Change::Kept, 0),
                (Change::Kept, 1),
                (Change::Added, 2),
            ]
        );
    }

    #[test]
    fn images_are_read_from_the_repository_only() {
        assert_eq!(
            repo_path("docs/guide.md", "shot.png").as_deref(),
            Some("docs/shot.png")
        );
        assert_eq!(
            repo_path("docs/guide.md", "./img/a%20b.png?raw=1#x").as_deref(),
            Some("docs/img/a b.png")
        );
        assert_eq!(
            repo_path("docs/guide.md", "../assets/logo.svg").as_deref(),
            Some("assets/logo.svg")
        );
        assert_eq!(
            repo_path("docs/guide.md", "/assets/logo.svg").as_deref(),
            Some("assets/logo.svg")
        );
        for elsewhere in [
            "https://example.org/x.png",
            "//example.org/x.png",
            "../../outside.png",
            "file:///etc/passwd",
            "C:/x.png",
            "\\\\server\\share\\x.png",
            "%5C%5Cserver%5Cshare%5Cx.png",
            "data:image/png;base64,AAAA",
            "",
        ] {
            assert_eq!(repo_path("docs/guide.md", elsewhere), None, "{elsewhere}");
        }
    }

    #[test]
    fn a_repository_image_stays_and_any_other_becomes_a_link() {
        let text = "![logo](logo.png) ![pixel](https://evil.example/p.png)\n\n[![badge](badge.svg)](https://ci.example)";
        let (safe, kept) = pr::safe_local_markdown(text, &mut |url| {
            (!url.contains("://")).then(|| format!("bytes://markdown/1/{url}"))
        });
        assert_eq!(
            safe,
            "![logo](bytes://markdown/1/logo.png) [pixel](https://evil.example/p.png)\n\n\
             [![badge](bytes://markdown/1/badge.svg)](https://ci.example)"
        );
        assert_eq!(kept.len(), 2);
    }

    /// A folder of the working tree that is a link reads nothing through it; a file in a plain
    /// folder is read.
    #[cfg(unix)]
    #[test]
    fn working_tree_images_never_go_through_a_linked_folder() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-md-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("docs")).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("docs/s.png"), "picture").map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink("/", dir.join("docs/l")).map_err(|e| e.to_string())?;
        let plain = working_tree_image(&dir, "docs/s.png").is_some();
        let linked = working_tree_image(&dir, "docs/l/etc/hostname").is_some();
        let missing = working_tree_image(&dir, "docs/none.png").is_some();
        // And as the rendered file's images are read: nothing through the link.
        let mut images = Images::default();
        let side = Side {
            rev: None,
            path: "docs/a.md".to_owned(),
        };
        let through_link = images.local(&dir, &side, "l/etc/hostname");
        let read_after_link = images.found.len();
        let beside = images.local(&dir, &side, "s.png");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(plain);
        assert!(!linked);
        assert!(!missing);
        assert_eq!((through_link, read_after_link), (None, 0));
        assert!(beside.is_some());
        assert_eq!(images.found.len(), 1);
        Ok(())
    }

    #[test]
    fn a_file_reads_with_its_pictures_on_each_side() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-rendered-md-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("docs")).map_err(|e| e.to_string())?;
        let git = |args: &[&str]| git::run(Some(&dir), args);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            git(args)?;
        }
        let write = |path: &str, text: &str| {
            std::fs::write(dir.join(path), text).map_err(|e| e.to_string())
        };
        write("docs/a.md", "# A\n\nOld text.\n\n![s](s.png)\n")?;
        write("docs/s.png", "old picture")?;
        git(&["add", "-A"])?;
        git(&["commit", "-qm", "one"])?;
        write("docs/a.md", "# A\n\nNew text.\n\n![s](s.png)\n")?;
        write("docs/s.png", "new picture")?;
        git(&["commit", "-qam", "two"])?;
        let key = DiffKey::Commit {
            hash: git(&["rev-parse", "HEAD"])?.trim().to_owned(),
            path: "docs/a.md".to_owned(),
        };
        let md = load(&dir, &key);
        let _ = std::fs::remove_dir_all(&dir);
        let md = md?;
        let changes: Vec<Change> = md.blocks.iter().map(|(c, _)| *c).collect();
        // The picture changed too: its block shows it before and after.
        assert_eq!(
            changes,
            [
                Change::Kept,
                Change::Removed,
                Change::Removed,
                Change::Added,
                Change::Added
            ]
        );
        assert!(md.file.contains("New text."));
        assert!(!md.before);
        assert_eq!(md.images.len(), 2);
        assert!(md.images.iter().all(|(uri, _)| uri.ends_with("/s.png")));
        Ok(())
    }
}
