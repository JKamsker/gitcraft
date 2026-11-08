//! Images in a diff: both versions read from git and shown before / after.

use std::path::Path;
use std::sync::Arc;

use egui::{Ui, load::Bytes, vec2};

use crate::git::{self, Area};
use crate::repo::DiffKey;
use crate::theme::{self, Tokens};

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "svg"];

pub struct ImagePair {
    pub old: Option<Arc<[u8]>>,
    pub new: Option<Arc<[u8]>>,
    /// Makes the image URIs unique to this diff.
    pub id: String,
    pub svg: bool,
}

pub(super) fn is_image(path: &str) -> bool {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.as_str()))
}

/// The old name of a renamed file, from its patch's header (unless git had to quote it).
pub(super) fn renamed_from(header: &str) -> Option<&str> {
    header
        .lines()
        .find_map(|l| l.strip_prefix("rename from "))
        .filter(|p| !p.starts_with('"'))
}

/// The pictures before and after; the one before under its old name when the file was renamed.
pub(super) fn load_images(
    repo: &Path,
    key: &DiffKey,
    path: &str,
    renamed_from: Option<&str>,
) -> ImagePair {
    let get = |rev: Option<&str>, path: &str| {
        git::file_at(repo, rev, path)
            .ok()
            .map(|b| Arc::from(b.into_boxed_slice()))
    };
    let old_path = match key {
        DiffKey::Work(file) => file.old_path.as_deref(),
        _ => None,
    }
    .or(renamed_from)
    .unwrap_or(path);
    let old = |rev: &str| get(Some(rev), old_path);
    let (old, new) = match key {
        DiffKey::Commit { hash, .. } => (old(&format!("{hash}^")), get(Some(hash), path)),
        DiffKey::Work(file) => match file.area {
            Area::Staged => (old("HEAD"), get(Some(""), path)),
            Area::Unstaged => (get(Some(""), path), get(None, path)),
            Area::Untracked | Area::Conflicted => (None, get(None, path)),
        },
        DiffKey::Range { from, to, .. } => (old(from), get(to.as_deref(), path)),
        // git's own merge of a picture is nowhere to be read.
        DiffKey::Remerge { hash, .. } => (None, get(Some(hash), path)),
    };
    let id = image_id(old.as_deref(), new.as_deref());
    ImagePair {
        old,
        new,
        id,
        svg: Path::new(path)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("svg")),
    }
}

/// Names the pictures by what they hold: egui keeps an image by its URI for good, so the same
/// name for new contents would show the old picture, and a new name for each read of the same
/// one would keep another copy every time the diff is read again.
pub(super) fn image_id(old: Option<&[u8]>, new: Option<&[u8]>) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (old, new).hash(&mut h);
    format!("{:016x}", h.finish())
}

pub(super) fn image_view(ui: &mut Ui, images: &ImagePair) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(12.0);
    if images.svg {
        ui.label(crate::i18n::text("repo-core-1849"));
    }
    ui.columns(2, |cols| {
        for (col, (label, bytes, side)) in cols.iter_mut().zip([
            (crate::i18n::text("repo-core-1564"), &images.old, "old"),
            (crate::i18n::text("repo-core-1565"), &images.new, "new"),
        ]) {
            col.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .font(theme::semibold(12.0))
                        .color(t.text_dim),
                );
                ui.add_space(6.0);
                match bytes {
                    Some(bytes) => {
                        let extension = if images.svg { "svg" } else { "png" };
                        let uri = format!("bytes://diff/{}/{side}.{extension}", images.id);
                        let max = vec2(ui.available_width() - 16.0, ui.available_height() - 40.0);
                        ui.add(
                            egui::Image::from_bytes(uri, Bytes::Shared(bytes.clone()))
                                .max_size(max)
                                .fit_to_original_size(1.0)
                                .bg_fill(t.field),
                        );
                        ui.label(
                            egui::RichText::new(crate::i18n::message(
                                "repo-core-1851",
                                &[("div-ceil", format!("{}", bytes.len().div_ceil(1024)).into())],
                            ))
                            .color(t.text_faint)
                            .small(),
                        );
                    }
                    None => {
                        let note = if side == "old" {
                            crate::i18n::text("repo-core-1695")
                        } else {
                            crate::i18n::text("repo-core-1696")
                        };
                        ui.label(egui::RichText::new(note).color(t.text_faint));
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod svg_tests {
    use super::*;
    #[test]
    fn svg_uses_the_rendered_image_path_and_keeps_its_code_patch() -> Result<(), String> {
        let r = crate::git::file_comparison::tests::Repo::new()?;
        r.write("icon.svg",r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="5" height="5"/></svg>"#)?;
        r.commit()?;
        r.write("icon.svg",r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><circle r="5"/></svg>"#)?;
        let key = DiffKey::Work(git::StatusFile {
            path: "icon.svg".into(),
            old_path: None,
            area: Area::Unstaged,
            code: 'M',
        });
        let data = super::super::load(&r.0, &key, git::DiffOptions::default(), true)?;
        let image = data.image.as_ref().ok_or("no SVG preview")?;
        assert!(image.svg && image.old.is_some() && image.new.is_some());
        assert!(data.lines.iter().any(|l| l.text.contains("circle")));
        assert!(data.partial);
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        crate::svg::install(&ctx);
        ctx.include_bytes(
            "bytes://test.svg",
            image.new.as_ref().ok_or("new")?.to_vec(),
        );
        assert!(
            ctx.try_load_image("bytes://test.svg", egui::load::SizeHint::default())
                .is_ok()
        );
        Ok(())
    }
    #[test]
    fn renamed_svg_with_git_quoted_paths_keeps_both_rendered_versions() -> Result<(), String> {
        let r = crate::git::file_comparison::tests::Repo::new()?;
        let old = "old\tü.svg";
        let new = "new ü.svg";
        let bytes=br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="5" height="5"/></svg>"#;
        r.write(old, bytes)?;
        r.commit()?;
        r.git(&["mv", old, new])?;
        r.commit()?;
        let data = super::super::load(
            &r.0,
            &DiffKey::Commit {
                hash: "HEAD".into(),
                path: new.into(),
            },
            git::DiffOptions::default(),
            true,
        )?;
        // Pure renames intentionally have no pickable hunk header in DiffData.
        let raw = git::file_diff(&r.0, "HEAD", new, git::DiffOptions::default())?;
        assert!(raw.contains("rename from \""));
        let image = data.image.as_ref().ok_or("SVG preview")?;
        assert!(image.svg);
        assert_eq!(image.old.as_deref(), Some(bytes.as_slice()));
        assert_eq!(image.new.as_deref(), Some(bytes.as_slice()));
        Ok(())
    }
}
