//! Synthetic replies: no repository reads or worker processes are needed for these races.
use super::*;
use crate::repo::Msg;

fn tab() -> RepoTab {
    RepoTab::new_deferred(
        "/nonexistent/file-view-generation-test".into(),
        &egui::Context::default(),
    )
}

fn view(rev: Option<&str>) -> FileView {
    FileView::new("same.txt", rev.map(str::to_owned), FileTab::History)
}

fn replies(view: &FileView, label: &str) -> [Msg; 3] {
    let (g, p, r) = (view.generation, view.path.clone(), view.rev.clone());
    [
        Msg::FileHistory(g, p.clone(), r.clone(), Err(label.into())),
        Msg::Blame(g, p.clone(), r.clone(), Err(label.into())),
        Msg::FileText(g, p, r, Err(label.into())),
    ]
}

fn deliver(tab: &mut RepoTab, messages: [Msg; 3]) {
    for msg in messages {
        tab.handle(msg, &egui::Context::default());
    }
}

fn assert_label(view: &FileView, label: &str) {
    assert_eq!(view.history.as_ref().unwrap().as_ref().unwrap_err(), label);
    assert_eq!(view.blame.as_ref().unwrap().as_ref().err().unwrap(), label);
    assert_eq!(view.text.as_ref().unwrap().as_ref().err().unwrap(), label);
}

#[test]
fn mutable_refresh_clears_errors_and_rejects_stale_replies() {
    for rev in [
        None,
        Some(""),
        Some("HEAD"),
        Some("main"),
        Some("v1"),
        Some("abcdef0"),
    ] {
        let mut tab = tab();
        tab.file_view = Some(view(rev));
        let old = replies(tab.file_view.as_ref().unwrap(), "old");
        deliver(&mut tab, replies(&view(rev), "unrelated"));
        let cached = replies(tab.file_view.as_ref().unwrap(), "cached error");
        deliver(&mut tab, cached);
        let v = tab.file_view.as_mut().unwrap();
        assert!(v.invalidate_mutable());
        assert!(v.needs(FileTab::History) && v.needs(FileTab::Blame) && v.needs(FileTab::File));
        let fresh = replies(v, "fresh");
        deliver(&mut tab, fresh);
        deliver(&mut tab, old);
        assert_label(tab.file_view.as_ref().unwrap(), "fresh");
    }
}

#[test]
fn close_reopen_and_a_b_a_reject_previous_openings() {
    for via_b in [false, true] {
        let mut tab = tab();
        tab.file_view = Some(view(Some("HEAD")));
        let old = replies(tab.file_view.as_ref().unwrap(), "old A");
        tab.file_view = None;
        if via_b {
            let mut b = view(Some("HEAD"));
            b.path = "other.txt".into();
            tab.file_view = Some(b);
        }
        tab.file_view = Some(view(Some("HEAD")));
        deliver(&mut tab, old);
        let v = tab.file_view.as_ref().unwrap();
        assert!(v.history.is_none() && v.blame.is_none() && v.text.is_none());
    }
}

#[test]
fn ordinary_pane_switches_and_parallel_tabs_preserve_their_own_replies() {
    let mut a = tab();
    let mut b = tab();
    a.file_view = Some(view(None));
    b.file_view = Some(view(None));
    let a_reply = replies(a.file_view.as_ref().unwrap(), "A");
    let b_reply = replies(b.file_view.as_ref().unwrap(), "B");
    let generation = a.file_view.as_ref().unwrap().generation;
    // Preload panes so switching needs no background IO.
    deliver(&mut a, replies(b.file_view.as_ref().unwrap(), "wrong tab"));
    let cached = replies(a.file_view.as_ref().unwrap(), "cached");
    deliver(&mut a, cached);
    for pane in [FileTab::Blame, FileTab::File, FileTab::History] {
        a.open_file_view("same.txt".into(), None, pane, &egui::Context::default());
        assert_eq!(a.file_view.as_ref().unwrap().generation, generation);
    }
    deliver(&mut a, a_reply);
    deliver(&mut b, b_reply);
    assert_label(a.file_view.as_ref().unwrap(), "A");
    assert_label(b.file_view.as_ref().unwrap(), "B");
}

#[test]
fn full_object_ids_retain_their_cache_and_generation() {
    for len in [40, 64] {
        let mut v = view(Some(&"a".repeat(len)));
        v.history = Some(Ok(Vec::new()));
        let generation = v.generation;
        assert!(!v.invalidate_mutable());
        assert_eq!(v.generation, generation);
        assert!(v.history.is_some());
    }
}

#[test]
fn line_history_replies_cannot_cross_refresh_or_identical_reopening() {
    for reopen in [false, true] {
        let mut tab = tab();
        let mut v = view(None);
        v.lines = Some((git::LinesAt::Index, (4, 9)));
        let old =
            line_history::history_reply(v.generation, v.path.clone(), git::LinesAt::Index, (4, 9));
        tab.file_view = Some(v);
        if reopen {
            let mut v = view(None);
            v.lines = Some((git::LinesAt::Index, (4, 9)));
            tab.file_view = Some(v);
        } else {
            tab.file_view.as_mut().unwrap().invalidate_mutable();
        }
        tab.handle(
            Msg::Then(Box::new(move |tab, ctx| old(tab, Err("stale".into()), ctx))),
            &egui::Context::default(),
        );
        let v = tab.file_view.as_ref().unwrap();
        assert!(v.history.is_none());
        assert_eq!(v.lines, Some((git::LinesAt::Index, (4, 9))));
        let fresh =
            line_history::history_reply(v.generation, v.path.clone(), git::LinesAt::Index, (4, 9));
        tab.handle(
            Msg::Then(Box::new(move |tab, ctx| fresh(tab, Ok(Vec::new()), ctx))),
            &egui::Context::default(),
        );
        assert!(
            tab.file_view
                .as_ref()
                .unwrap()
                .history
                .as_ref()
                .unwrap()
                .is_ok()
        );
    }
}
