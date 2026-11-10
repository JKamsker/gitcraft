use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_inert_scan_progress_result_and_error_paint_preserve_original_open_request_and_state() {
    crate::i18n::set_language("de");
    let (_, rx) = channel();
    let scan = RepoScan {
        root: "/opaque/root".into(),
        rx,
        progress: Progress {
            folders: 2345,
            repos: 6,
        },
        result: None,
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, state: &mut (RepoScan, Option<PathBuf>)| {
            if !fonts.replace(true) {
                theme::install_fonts(ui.ctx());
                return;
            }
            if let Some(ScanClick::Open(path)) = scan_body(ui, &state.0, &Tokens::get(ui.ctx())) {
                state.1 = Some(path);
            }
        },
        (scan, None),
    );
    h.run_steps(3);
    h.get_by_label("2.345 Ordner durchsucht, 6 Repositorys gefunden…");
    let path = PathBuf::from("/opaque/repo 日本語");
    h.state_mut().0.result = Some(Ok(Found {
        repos: vec![path.clone()],
        stopped: Some(Stopped::Asked),
    }));
    h.run();
    h.get_by_label("Angehalten: Bisher 1 gefunden.");
    h.get_by_label("Öffnen").click();
    h.run();
    assert_eq!(h.state().1, Some(path.clone()));
    assert_eq!(
        h.state()
            .0
            .result
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(|f| f.repos.as_slice()),
        Some([path.clone()].as_slice())
    );
    assert!(!h.state().0.cancel.load(Ordering::Relaxed));
    let raw = unreadable(
        Path::new("/opaque/日本語"),
        &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    );
    assert_eq!(raw, "gitcraft isn't allowed to look in /opaque/日本語.");
    h.state_mut().0.result = Some(Err(raw.clone()));
    h.run();
    h.get_by_label("gitcraft darf /opaque/日本語 nicht durchsuchen.");
    assert_eq!(
        h.state().0.result.as_ref().and_then(|r| r.as_ref().err()),
        Some(&raw)
    );
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label(&raw);
}

#[test]
fn actual_recent_add_counts_keep_order_and_format_all_notes_in_the_selected_language() {
    crate::i18n::set_language("de");
    let mut recent = vec![PathBuf::from("/literal/old")];
    let found = [
        PathBuf::from("/literal/old"),
        PathBuf::from("/literal/new"),
        PathBuf::from("/literal/no-room"),
    ];
    let counts = add_to_recent(&mut recent, &found, 2);
    assert_eq!(counts, (1, 1, 1));
    assert_eq!(
        recent,
        [PathBuf::from("/literal/old"), PathBuf::from("/literal/new")]
    );
    assert_eq!(
        added_message(1, 0, 0),
        "1 Repository zur Liste zuletzt verwendeter Repositorys hinzugefügt"
    );
    assert!(added_message(2, 1, 0).ends_with("(1 bereits vorhanden)"));
    assert!(added_message(0, 0, 2).contains("2 Repositorys hatten keinen Platz"));
    assert!(
        added_message(counts.0, counts.1, counts.2)
            .contains("1 bereits vorhanden; 1 Repository hatte keinen Platz")
    );
    assert!(added_message(1234, 0, 0).starts_with("1.234 Repositorys"));
    assert!(stopped_message(Stopped::Folders, 0).contains("50.000"));
    assert!(display_error("External scan diagnostic 日本語").is_none());
    crate::i18n::set_language("en-US");
    assert_eq!(
        added_message(1, 1, 1),
        format!(
            "Added 1 repository to the recent list (1 was already there; 1 repository didn't fit: it holds {MAX_RECENT}, remove some to make room)"
        )
    );
}
