use super::*;
use crate::repo::find::Query;
fn painted(output: &egui::FullOutput) -> String {
    output
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Text(t) = &s.shape {
                Some(t.galley.text())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn markdown_searches_visible_text_across_inline_styles_and_not_link_destinations() {
    let mut doc = Document::default();
    doc.markdown(
        "# Héllo **world**\n\n[visible label](https://hidden.invalid/secret) and `code`",
        Color32::WHITE,
    );
    let text = doc
        .rows
        .iter()
        .map(|j| j.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Héllo world"), "{text}");
    assert!(text.contains("visible label and code"), "{text}");
    assert!(!text.contains("hidden.invalid"));
    assert!(!text.contains("**"));
    assert!(doc.rows[0].sections.len() > 1);
}
#[test]
fn rendered_reader_marks_steps_and_invalidates_exact_source() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut find = Find {
        open: true,
        query: Query {
            text: "café".into(),
            ..Query::default()
        },
        ..Find::default()
    };
    for _ in 0..2 {
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            find.rendered(ui, "a".into(), |t| {
                let mut doc = Document::default();
                doc.markdown("**café** then café", t.text);
                doc
            })
        });
        assert!(painted(&output).contains("café then café"));
        assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.fill==Tokens::get(&ctx).warning.gamma_multiply(0.6))));
        output.drop_without_applying_deltas();
    }
    assert_eq!(find.count(), 2);
    assert_eq!(find.current().map(|h| h.start), Some(0));
    find.step(1);
    assert_eq!(find.current().map(|h| h.start), Some(10));
    find.step(-1);
    assert_eq!(find.current().map(|h| h.start), Some(0));
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        find.rendered(ui, "b".into(), |t| {
            let mut doc = Document::default();
            doc.text("new content", t.text);
            doc
        })
    });
    output.drop_without_applying_deltas();
    assert_eq!(find.count(), 0);
    find.close();
    assert!(find.rendered_document.is_none());
}
#[test]
fn long_unicode_rows_keep_all_text_and_section_byte_ranges() {
    let mut doc = Document::default();
    let text = "é".repeat(crate::repo::highlight::MAX_LINE_CHARS + 10);
    doc.text(&text, Color32::WHITE);
    doc.bound_rows();
    assert_eq!(doc.rows.len(), 1);
    assert_eq!(doc.segments.len(), 2);
    assert_eq!(
        doc.rows.iter().map(|j| j.text.as_str()).collect::<String>(),
        text
    );
    for row in &doc.rows {
        for section in &row.sections {
            assert!(
                row.text
                    .get(section.byte_range.start.0..section.byte_range.end.0)
                    .is_some()
            );
        }
    }
}
