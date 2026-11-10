use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn retained_home_errors_paint_en_de_en_from_original_source_without_io() {
    crate::i18n::set_language("en-US");
    let path = Path::new("/opaque/日本語 isn't in a git repository");
    let detail = "native 日本語\n: $(opaque) git init failed: literal";
    let errors = vec![
        AppError::init(detail.into()),
        super::super::dropped::retained_folder_error(path, "FATAL: NOT A GIT REPOSITORY"),
        super::super::dropped::retained_folder_error(path, detail),
        AppError::unsupported("日本語.patch.txt".into()),
        AppError::patch_target(false, "日本語.patch"),
        AppError::patch_target(true, "literal.patch"),
    ];
    let raw: Vec<_> = errors.iter().map(|e| e.raw.clone()).collect();
    let sources: Vec<_> = errors.iter().map(|e| e.presentation.clone()).collect();
    assert_eq!(sources[0], Some(HomeError::Init(detail.into())));
    assert_eq!(
        sources[1],
        Some(HomeError::Folder {
            folder: path.into(),
            error: "FATAL: NOT A GIT REPOSITORY".into(),
            not_repo: true,
        })
    );
    assert_eq!(
        sources[2],
        Some(HomeError::Folder {
            folder: path.into(),
            error: detail.into(),
            not_repo: false,
        })
    );
    assert_eq!(
        sources[3],
        Some(HomeError::Unsupported("日本語.patch.txt".into()))
    );
    assert_eq!(
        sources[4],
        Some(HomeError::PatchTarget {
            has_repo: false,
            name: "日本語.patch".into()
        })
    );
    assert_eq!(
        sources[5],
        Some(HomeError::PatchTarget {
            has_repo: true,
            name: "literal.patch".into()
        })
    );
    assert_eq!(raw, [
        format!("git init failed: {detail}"),
        "日本語 isn't in a git repository isn't in a git repository: drop a repository's folder to open it".into(),
        format!("Could not open 日本語 isn't in a git repository: {detail}"),
        "Can't open 日本語.patch.txt: drop a folder to open it, or a .patch, .diff, .eml or .mbox file to apply it".into(),
        "Applying 日本語.patch needs a repository open first: open one, then drop the patch on it".into(),
        "To apply literal.patch, show its repository's tab, then drop the patch there".into(),
    ]);
    // Only supplied error values reach the exact label function used by App::home.
    let mut h = Harness::new_ui_state(
        |ui, errors: &mut Vec<AppError>| {
            for error in errors {
                error_label(ui, egui::Color32::RED, error);
            }
        },
        errors,
    );
    h.run();
    for expected in &raw {
        h.get_by_label(expected);
    }
    crate::i18n::set_language("de");
    h.step();
    for expected in [
        format!("git init fehlgeschlagen: {detail}"),
        "日本語 isn't in a git repository liegt nicht in einem Git-Repository: Ziehe einen Repository-Ordner hierher, um ihn zu öffnen".into(),
        format!("日本語 isn't in a git repository konnte nicht geöffnet werden: {detail}"),
        "日本語.patch.txt kann nicht geöffnet werden: Ziehe einen Ordner hierher, um ihn zu öffnen, oder eine .patch-, .diff-, .eml- oder .mbox-Datei, um sie anzuwenden".into(),
        "Um 日本語.patch anzuwenden, muss zuerst ein Repository geöffnet sein: Öffne eines und lege den Patch dort ab".into(),
        "Zeige den Tab des zugehörigen Repositorys und lege literal.patch dort ab, um den Patch anzuwenden".into(),
    ] { h.get_by_label(&expected); }
    let emitted = h.state()[0].delivery_text();
    assert_eq!(emitted, format!("git init fehlgeschlagen: {detail}"));
    for (i, error) in h.state().iter().enumerate() {
        assert_eq!(error.raw, raw[i]);
        assert_eq!(error.presentation, sources[i]);
    }
    crate::i18n::set_language("en-US");
    h.step();
    for expected in &raw {
        h.get_by_label(expected);
    }
    assert_eq!(emitted, format!("git init fehlgeschlagen: {detail}"));
    for (i, error) in h.state().iter().enumerate() {
        assert_eq!(error.raw, raw[i]);
        assert_eq!(error.presentation, sources[i]);
    }
}

#[test]
fn plain_replacement_clearing_and_mismatch_never_recover_source_from_text() -> Result<(), String> {
    crate::i18n::set_language("de");
    let original = AppError::init("fixture native error".into());
    let raw = original.raw.clone();
    let mut retained = Some(original);
    assert_eq!(
        retained.as_ref().ok_or("missing test value")?.display(),
        "git init fehlgeschlagen: fixture native error"
    );
    // Identical raw text from an unrelated write has no provenance.
    retained = Some(raw.clone().into());
    assert!(
        retained
            .as_ref()
            .ok_or("missing test value")?
            .presentation
            .is_none()
    );
    assert_eq!(
        retained.as_ref().ok_or("missing test value")?.display(),
        raw
    );
    retained = None;
    assert!(retained.is_none());
    retained = Some(raw.clone().into());
    assert_eq!(retained.as_deref(), Some(raw.as_str()));
    assert_eq!(
        retained.as_ref().ok_or("missing test value")?.display(),
        raw
    );
    for lookalike in [
        "git init fehlgeschlagen: fixture native error",
        "Could not open y: fixture native error",
        "y isn't in a git repository: drop a repository's folder to open it",
        "Can't open y: drop a folder to open it, or a .patch, .diff, .eml or .mbox file to apply it",
        "Applying y needs a repository open first: open one, then drop the patch on it",
        "To apply y, show its repository's tab, then drop the patch there",
        "external 日本語\n: unknown",
    ] {
        let plain: AppError = lookalike.to_owned().into();
        assert!(plain.presentation.is_none());
        assert_eq!(plain.display(), lookalike);
    }
    // The existing finite ordinary-error paint fallback is preserved.
    assert_eq!(
        AppError::from("it is a symbolic link".to_owned()).display(),
        "Dies ist ein symbolischer Link."
    );
    // Production exposes only immutable Deref; simulate impossible stale metadata here.
    let mut mismatched = AppError::init("fixture native error".into());
    mismatched.raw = "it is a symbolic link".into();
    assert_eq!(mismatched.display(), "it is a symbolic link");
    assert_eq!(mismatched.delivery_text(), "it is a symbolic link");
    crate::i18n::set_language("en-US");

    Ok(())
}
