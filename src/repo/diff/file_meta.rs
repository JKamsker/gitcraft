//! What a diff changes of a file besides its lines: its mode (the executable bit, a file
//! becoming a link) and its name (a rename or copy). Git says so before the first hunk; the
//! header shows it apart from the content edits, so a mode-only change isn't an empty diff.

use egui::{RichText, Ui};

use crate::theme::Tokens;

/// The metadata lines of one file's patch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileMeta {
    /// `old mode` → `new mode`, when the patch changes the mode.
    pub mode: Option<(String, String)>,
    /// The old name of a renamed file.
    pub renamed_from: Option<String>,
    /// The name of the file a copied file was copied from.
    pub copied_from: Option<String>,
    /// `similarity index` of a rename or copy, in percent.
    pub similarity: Option<u8>,
}

impl FileMeta {
    pub fn is_empty(&self) -> bool {
        self.mode.is_none() && self.renamed_from.is_none() && self.copied_from.is_none()
    }
}

/// Reads the metadata from the part of `text` (one file's patch) before its first hunk.
pub(super) fn read(text: &str) -> FileMeta {
    let mut meta = FileMeta::default();
    let (mut old_mode, mut new_mode) = (None, None);
    for line in text.split_terminator('\n') {
        let line = line.trim_end_matches('\r');
        if line.starts_with("@@") {
            break;
        }
        let value = |prefix: &str| line.strip_prefix(prefix).map(str::to_owned);
        if let Some(v) = value("old mode ") {
            old_mode = Some(v);
        } else if let Some(v) = value("new mode ") {
            new_mode = Some(v);
        } else if let Some(v) = value("rename from ") {
            meta.renamed_from = Some(crate::git::unquote_path(&v));
        } else if let Some(v) = value("copy from ") {
            meta.copied_from = Some(crate::git::unquote_path(&v));
        } else if let Some(v) = line.strip_prefix("similarity index ") {
            meta.similarity = v.trim_end_matches('%').parse().ok();
        }
    }
    meta.mode = old_mode.zip(new_mode);
    meta
}

/// What a mode is, in words: `100755` is an executable file.
fn mode_name(mode: &str) -> &'static str {
    match mode {
        "100755" => crate::i18n::text("file-metadata-mode-executable"),
        "120000" => crate::i18n::text("file-metadata-mode-link"),
        "160000" => crate::i18n::text("file-metadata-mode-submodule"),
        _ => crate::i18n::text("file-metadata-mode-regular"),
    }
}

/// The lines saying what changed besides the content.
pub(super) fn lines(meta: &FileMeta) -> Vec<String> {
    let mut out = Vec::new();
    if let Some((old, new)) = &meta.mode {
        out.push(crate::i18n::message(
            "file-metadata-diff-mode",
            &[
                ("old", old.clone().into()),
                ("new", new.clone().into()),
                ("old-kind", mode_name(old).into()),
                ("new-kind", mode_name(new).into()),
            ],
        ));
    }
    let similar = meta
        .similarity
        .map_or_else(|| "none".to_owned(), |n| n.to_string());
    for (from, key) in [
        (&meta.renamed_from, "file-metadata-diff-renamed"),
        (&meta.copied_from, "file-metadata-diff-copied"),
    ] {
        if let Some(from) = from {
            out.push(crate::i18n::message(
                key,
                &[
                    ("from", from.clone().into()),
                    ("similarity", similar.clone().into()),
                ],
            ));
        }
    }
    out
}

/// The metadata of the diff shown, under its origin; nothing when there is none.
pub(super) fn show(ui: &mut Ui, meta: &FileMeta) {
    if meta.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    for line in lines(meta) {
        ui.add(egui::Label::new(RichText::new(&line).small().color(t.accent)).truncate())
            .on_hover_text(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_change_and_rename_are_read_before_the_first_hunk() {
        let text = "diff --git a/Old.sh b/new.sh\nold mode 100644\r\nnew mode 100755\n\
                    similarity index 92%\nrename from Old.sh\nrename to new.sh\n\
                    @@ -1 +1 @@\n-old mode 1\n+rename from x\n";
        let meta = read(text);
        assert_eq!(meta.mode, Some(("100644".into(), "100755".into())));
        assert_eq!(meta.renamed_from.as_deref(), Some("Old.sh"));
        assert_eq!(meta.similarity, Some(92));
        crate::i18n::set_language("en-US");
        let shown = lines(&meta);
        assert_eq!(shown.len(), 2);
        assert!(shown[0].contains("100644") && shown[0].contains("executable"));
        assert!(shown[1].contains("Old.sh") && shown[1].contains("92"));
    }

    #[test]
    fn content_only_and_new_files_have_no_metadata_line() {
        let edit = "diff --git a/a b/a\nindex 1..2 100644\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n";
        assert!(read(edit).is_empty());
        let new = "diff --git a/a b/a\nnew file mode 100755\nindex 0..1\n";
        assert!(read(new).is_empty());
        let copy = read("diff --git a/a b/c\ncopy from \"\\303\\274.txt\"\ncopy to c\n");
        assert_eq!(copy.copied_from.as_deref(), Some("ü.txt"));
        assert_eq!(lines(&copy).len(), 1);
    }
}
