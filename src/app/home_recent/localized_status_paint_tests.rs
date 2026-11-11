use super::*;
use crate::app::recent_status::Head;

#[test]
fn actual_recent_first_line_paints_typed_status_en_de_en_without_changing_raw_state() {
    let cases = [
        (
            crate::app::recent_status::parse(
                "# branch.oid (initial)\n# branch.head topic: 色 {$x}\n",
            ),
            "topic: 色 {$x} · noch keine Commits",
        ),
        (
            RecentStatus::Repo {
                head: Head::Detached("0123456".into()),
                dirty: false,
            },
            "losgelöst bei 0123456",
        ),
        (RecentStatus::NotFound, "Ordner nicht gefunden"),
        (RecentStatus::NotRepo, "kein Repository mehr"),
        (RecentStatus::TimedOut, "hat nicht rechtzeitig geantwortet"),
        (
            RecentStatus::Failed("opaque provider: not a repository any more".into()),
            "konnte nicht gelesen werden",
        ),
        (
            RecentStatus::Repo {
                head: Head::Branch("folder not found".into()),
                dirty: true,
            },
            "folder not found",
        ),
    ];
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    ctx.run_ui(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();
    for (status, german) in cases {
        let original = status.clone();
        let raw = status.label();
        let reason = status.reason().map(str::to_owned);
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            let expected = if locale == "de" { german } else { raw.as_str() };
            let out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1200., 100.))),
                    ..Default::default()
                },
                |ui| {
                    first_line(
                        ui,
                        ui.painter(),
                        Path::new("/supplied/repo"),
                        Some(&status),
                        pos2(40., 30.),
                        1150.,
                    );
                },
            );
            assert!(out.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text() == expected)),"missing {expected}");
            out.drop_without_applying_deltas();
            assert_eq!(status, original);
            assert_eq!(status.label(), raw);
            assert_eq!(status.reason(), reason.as_deref());
        }
    }
}
