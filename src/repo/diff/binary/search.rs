use super::BinaryPair;
use crate::repo::find::rendered::Document;
use crate::theme::Tokens;

pub(in crate::repo::diff) fn document(pair: &BinaryPair, t: &Tokens) -> Document {
    let mut doc = Document::default();
    for (before, label, side) in [
        (true, crate::i18n::text("repo-core-1564"), &pair.old),
        (false, crate::i18n::text("repo-core-1565"), &pair.new),
    ] {
        let Some(side) = side else { continue };
        doc.text(
            &format!(
                "{label} · {} · {}",
                side.kind
                    .as_deref()
                    .unwrap_or(crate::i18n::text("repo-core-1567")),
                side.size
                    .map(crate::git::human_size)
                    .unwrap_or_else(|| crate::i18n::text("repo-core-1568").into())
            ),
            t.text,
        );
        for (key, value) in &side.facts {
            doc.text(&format!("{key}: {value}"), t.text_dim);
        }
        if let Some(text) = &side.text {
            doc.text(text, if before { t.danger } else { t.success });
        }
        for note in &side.text_gaps {
            doc.text(note, t.warning);
        }
        for (i, bytes) in side.head.chunks(16).enumerate() {
            let hex = bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            let ascii = bytes
                .iter()
                .map(|b| {
                    if b.is_ascii_graphic() || *b == b' ' {
                        *b as char
                    } else {
                        '.'
                    }
                })
                .collect::<String>();
            doc.text(
                &format!("{label} {offset:08x}  {hex}  {ascii}", offset = i * 16),
                t.text_dim,
            );
        }
    }
    if let Some(members) = &pair.members {
        for member in members {
            doc.text(
                &crate::i18n::message(
                    "repo-core-1571",
                    &[
                        ("name", format!("{}", member.name).into()),
                        (
                            "old",
                            member
                                .old
                                .map(crate::git::human_size)
                                .unwrap_or_else(|| crate::i18n::text("repo-core-2228").to_owned())
                                .into(),
                        ),
                        (
                            "new",
                            member
                                .new
                                .map(crate::git::human_size)
                                .unwrap_or_else(|| crate::i18n::text("repo-core-2228").to_owned())
                                .into(),
                        ),
                    ],
                ),
                t.text,
            );
        }
    }
    for note in [&pair.text_note, &pair.members_note, &pair.error]
        .into_iter()
        .flatten()
    {
        doc.text(note, t.warning);
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn document_and_archive_text_is_searchable_without_raw_patch_markers() {
        let mut side = super::super::side("note.txt", 11, b"hello world");
        side.text = Some("Extracted document café".into());
        let mut pair = super::super::pair(None, Some(side), None);
        pair.members = Some(vec![super::super::MemberChange {
            name: "docs/guide.md".into(),
            old: Some(3),
            new: Some(9),
        }]);
        let doc = document(&pair, &Tokens::get(&egui::Context::default()));
        let text = doc
            .rows
            .iter()
            .map(|j| j.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Extracted document café"));
        assert!(text.contains("docs/guide.md"));
        assert!(!text.contains("Binary files differ"));
    }
}
