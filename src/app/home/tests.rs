use super::*;

/// A card's subtitle keeps clear of the card's sides: wrapped, then cut, never wider.
#[test]
fn card_subtitles_stay_inside_their_card() {
    let ctx = egui::Context::default();
    let cases = [
        (151.0, "An existing local repository"),
        (
            90.0,
            "A new repository (git init) with a much longer subtitle",
        ),
        (40.0, "Größe 日本語 repository"),
        (151.0, ""),
    ];
    let mut laid_out = Vec::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        for (width, text) in cases {
            let job = subtitle_job(text, width, egui::Color32::WHITE);
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            laid_out.push((width, text, galley.rect.width(), galley.rows.len()));
        }
    });
    // Nothing renders here: the font texture is left unapplied on purpose.
    output.textures_delta.clear();
    assert_eq!(laid_out.len(), cases.len());
    for (width, text, wide, rows) in laid_out {
        assert!(
            wide <= subtitle_width(width) + 0.5,
            "{text:?} at {width}: {wide}"
        );
        assert!(rows <= 2, "{text:?}: {rows} rows");
    }
}
