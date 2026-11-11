use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_inert_tool_paint_and_byte_previews_follow_selected_language() {
    // Deliberately bypass ToolApp::new: these empty channels never submit a job.
    // No snapshot reader, native app, repository, filesystem or process is started.
    let (_loading_sender, loading) = mpsc::channel();
    let (_saving_sender, saving) = mpsc::channel();
    let mut tool = ToolApp {
        active: Arc::new(AtomicBool::new(true)),
        outcome: Arc::new(AtomicU8::new(1)),
        loading: Some(loading),
        loaded: None,
        saving: Some(saving),
        error: Some("opaque native payload: ' $() & ü\n{}".into()),
        finished: false,
    };
    let binary = Snapshot {
        path: "literal ' $() & ü\npath".into(),
        bytes: vec![0, 0xff],
        stamp: None,
        handle: None,
        access: None,
    };
    let text = Snapshot {
        path: "text".into(),
        bytes: "unverändert ' $() & ü\n{}".as_bytes().to_vec(),
        stamp: None,
        handle: None,
        access: None,
    };
    let long = Snapshot {
        path: "long-text".into(),
        bytes: "ü".repeat(16_001).into_bytes(),
        stamp: None,
        handle: None,
        access: None,
    };
    let fonts = std::cell::Cell::new(false);
    crate::i18n::set_language("de");
    let mut h = Harness::builder().with_size([1400., 1200.]).build_ui(|ui| {
        if !fonts.replace(true) {
            crate::theme::install_fonts(ui.ctx());
            return;
        }
        tool.paint(ui);
        preview(ui, &binary);
        preview(ui, &text);
        preview(ui, &long);
    });
    h.run_steps(3);
    h.get_by_label("Dateien vergleichen");
    h.get_by_label("Dateiwerkzeug abbrechen");
    h.get_by_label("Dateikopien werden gelesen…");
    h.get_by_label("Der abgesicherte Speichervorgang wird abgeschlossen. Das Schließen wartet auf sein Ergebnis.");
    h.get_by_label("Mit Fehler schließen");
    h.get_by_label("opaque native payload: ' $() & ü\n{}");
    h.get_by_label("Binär/nicht UTF-8 · 2 Bytes\n00 ff ");
    h.get_by_label("unverändert ' $() & ü\n{}");
    h.get_by_label_contains("… Eingabevorschau gekürzt");
    assert!(h.query_by_label("Compare files").is_none());
    crate::i18n::set_language("en-US");
    h.run_steps(2);
    h.get_by_label("Compare files");
    h.get_by_label("Cancel file tool");
    h.get_by_label("Reading file snapshots…");
    h.get_by_label("Finishing the guarded save. Closing waits for its result.");
    h.get_by_label("Close with error");
    h.get_by_label("Binary/non-UTF8 · 2 bytes\n00 ff ");
    h.get_by_label_contains("… input preview truncated");
    assert!(h.query_by_label("Dateien vergleichen").is_none());
    drop(h);
    assert!(!tool.finished);
    assert_eq!(tool.outcome.load(Ordering::Acquire), 1);
}

#[test]
fn actual_disconnected_worker_adapter_localizes_without_starting_a_worker() -> Result<(), String> {
    for (locale, expected) in [
        ("en-US", "The file-tool worker stopped"),
        (
            "de",
            "Der Hintergrundauftrag des Dateiwerkzeugs wurde beendet",
        ),
    ] {
        crate::i18n::set_language(locale);
        let (sender, receiver) = mpsc::channel::<Result<(), String>>();
        drop(sender);
        let actual = receive(&receiver).ok_or("missing disconnection")?;
        assert_eq!(actual, Err(expected.to_owned()));
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
