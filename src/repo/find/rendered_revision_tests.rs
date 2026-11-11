use super::*;
use crate::repo::find::Query;
fn search(text: &str, query: Query) -> Find {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut find = Find {
        open: true,
        query,
        ..Default::default()
    };
    for _ in 0..2 {
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            find.rendered(ui, "logical-source".into(), |t| {
                let mut doc = Document::default();
                doc.text(text, t.text);
                doc
            })
        });
        output.drop_without_applying_deltas();
    }
    find
}
#[test]
fn logical_matches_cross_visual_cuts_without_inventing_regex_or_word_boundaries() {
    let text = format!("{}needlex", "é".repeat(4997));
    let find = search(
        &text,
        Query {
            text: "needle".into(),
            ..Default::default()
        },
    );
    assert_eq!(find.count(), 1);
    assert_eq!(
        find.current().map(|h| (h.start, h.end, h.row)),
        Some((4997, 5003, 0))
    );
    let find = search(
        &text,
        Query {
            text: "needle".into(),
            whole_word: true,
            ..Default::default()
        },
    );
    assert_eq!(find.count(), 0);
    let text = format!("{}needle", "x".repeat(5000));
    let find = search(
        &text,
        Query {
            text: "needle".into(),
            whole_word: true,
            ..Default::default()
        },
    );
    assert_eq!(find.count(), 0);
    let find = search(
        &text,
        Query {
            text: "^needle".into(),
            regex: true,
            ..Default::default()
        },
    );
    assert_eq!(find.count(), 0);
    let text = format!("{}needle{}", "x".repeat(4994), "x".repeat(10));
    let find = search(
        &text,
        Query {
            text: "needle$".into(),
            regex: true,
            ..Default::default()
        },
    );
    assert_eq!(find.count(), 0);
    let find = search(
        &format!("{} needle", "é".repeat(10005)),
        Query {
            text: "needle".into(),
            ..Default::default()
        },
    );
    assert_eq!(find.current().map(|h| (h.start, h.row)), Some((10006, 2)));
}
#[test]
fn near_limit_many_line_documents_refuse_explicitly_without_retaining_millions_of_jobs() {
    let source = "x\n".repeat(4 << 20);
    let mut doc = Document::default();
    doc.text(&source, Color32::WHITE);
    assert!(doc.limited);
    assert!(doc.rows.is_empty());
    let ctx = egui::Context::default();
    let mut find = Find {
        open: true,
        query: Query {
            text: "x".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        find.rendered(ui, "bounded".into(), |_| std::mem::take(&mut doc))
    });
    let text = output
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
        .join("\n");
    assert!(text.contains("no partial result count"), "{text}");
    assert_eq!(find.count(), 0);
    output.drop_without_applying_deltas();
}
