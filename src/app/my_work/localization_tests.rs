use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_my_work_results_translate_and_only_return_the_original_open_request() {
    crate::i18n::set_language("de");
    let path = PathBuf::from("/opaque/repo 日本語");
    let mut work = MyWork::default();
    work.looked.push(path.clone());
    work.seen.insert(
        path.clone(),
        Ok(Overview {
            changed: 1234,
            ..Default::default()
        }),
    );
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, opened: &mut Option<PathBuf>| {
            if !fonts.replace(true) {
                theme::install_fonts(ui.ctx());
                return;
            }
            let t = Tokens::get(ui.ctx());
            panel_header(ui, &work, &t);
            if let Some(Ask::Open(path)) = results(ui, &work, &[], &t) {
                *opened = Some(path);
            }
            assert!(
                work.pushes.is_empty() && work.push_queue.is_empty() && work.batches.is_empty()
            );
        },
        None,
    );
    h.run();
    h.get_by_label("Wo ist meine Arbeit?");
    h.get_by_label("DIESER RECHNER");
    h.get_by_label("1.234 Dateien mit nicht committeten Änderungen");
    h.get_by_label("Öffnen").click();
    h.run();
    assert_eq!(*h.state(), Some(path));
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label("Where's my work?");
    h.get_by_label("1,234 files with uncommitted changes");
}

#[test]
fn actual_push_control_returns_original_branch_and_tracking_flag_without_dispatch() {
    crate::i18n::set_language("de");
    let work = MyWork::default();
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, requested: &mut bool| {
            if !fonts.replace(true) {
                theme::install_fonts(ui.ctx());
                return;
            }
            let t = Tokens::get(ui.ctx());
            if let Some(Ask::Push(path, branch, tracked)) = push_button(
                ui,
                &work,
                Path::new("/opaque/repo"),
                "feature/日本語",
                false,
                &t,
            ) {
                assert_eq!(path, Path::new("/opaque/repo"));
                assert_eq!(branch, "feature/日本語");
                assert!(!tracked);
                *requested = true;
            }
            assert!(work.pushes.is_empty() && work.push_queue.is_empty());
        },
        false,
    );
    h.run();
    h.get_by_label("Pushen").click();
    h.run();
    assert!(*h.state());
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label("Push");
}

#[test]
fn work_descriptions_keep_push_classification_raw_and_unknown_errors_opaque() -> Result<(), String>
{
    crate::i18n::set_language("de");
    let item = Work::Unpushed {
        branch: "feature/日本語".into(),
        upstream: Some("origin/日本語".into()),
        commits: 1234,
        tracks: crate::git::Tracks::SameName,
    };
    assert_eq!(
        item.describe(),
        "feature/日本語: 1.234 Commits nicht nach origin/日本語 gepusht"
    );
    assert_eq!(item.push(), Some(("feature/日本語", true)));
    let other = Work::Unpushed {
        branch: "feature/日本語".into(),
        upstream: Some("origin/日本語".into()),
        commits: 1234,
        tracks: crate::git::Tracks::OtherName,
    };
    assert!(other.push().is_none());
    assert!(
        other
            .why_no_push()
            .ok_or("missing test value")?
            .contains("feature/日本語 folgt origin/日本語")
    );
    assert_eq!(
        Work::Stashes(vec!["wip 日本語".into(), "older".into()]).describe(),
        "2 Stashes, der neueste: wip 日本語"
    );
    let raw = "feature/日本語 has no upstream set any more";
    assert_eq!(
        across::display_error(raw),
        "Für feature/日本語 ist kein Upstream mehr festgelegt"
    );
    assert_eq!(raw, "feature/日本語 has no upstream set any more");
    assert_eq!(
        across::display_reason("fatal: not a git repository"),
        "kein Repository"
    );
    assert_eq!(
        across::short_reason("fatal: not a git repository"),
        "not a repository"
    );
    for raw in [
        "External diagnostic 日本語",
        "prefix feature has no upstream set any more",
        "no answer within banana; it may still be running",
    ] {
        assert_eq!(across::display_error(raw), raw);
    }
    assert_eq!(across::discover::Machine::Here.label(), "This computer");
    assert_eq!(
        across::discover::Machine::Here.display_label(),
        "Dieser Rechner"
    );
    crate::i18n::set_language("en-US");
    assert_eq!(across::display_error(raw), raw);

    Ok(())
}
