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
