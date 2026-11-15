use super::*;
fn browser() -> Browser {
    Browser::new(egui::ViewportId::ROOT)
}
fn page(next: Option<&str>) -> Page {
    Page {
        repositories: vec![],
        owners: vec![],
        next: next.map(str::to_owned),
        total: None,
        incomplete: false,
        rate: Default::default(),
    }
}
#[test]
fn browser_stops_non_advancing_pages_and_retains_rows_on_errors() {
    let mut browser = browser();
    browser.apply(Ok(page(Some("same"))));
    assert!(browser.next.is_none());
    browser.loaded = true;
    browser.apply(Err(DiscoveryError::Forbidden));
    assert!(browser.loaded);
    assert_eq!(browser.error, Some(DiscoveryError::Forbidden));
}
#[test]
fn browser_reset_retires_receiver_and_query_generation() {
    let mut browser = browser();
    let scope = crate::tasks::Scope::default();
    let cancel = scope.0.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    browser.pending = Some(Pending {
        tag: Tag {
            owner: browser.owner,
            serial: 0,
            connection: browser.session.generation,
            query: Query::Mine,
        },
        rx,
        _scope: scope,
    });
    browser.reset();
    assert!(cancel.is_cancelled());
    assert!(
        tx.send((
            Tag {
                owner: browser.owner,
                serial: 0,
                connection: browser.session.generation,
                query: Query::Mine
            },
            Ok(page(None))
        ))
        .is_err()
    );
    assert_eq!(browser.serial, 1);
}
#[test]
fn browser_disconnected_worker_releases_loading() {
    let mut browser = browser();
    let (tx, rx) = std::sync::mpsc::channel();
    drop(tx);
    browser.pending = Some(Pending {
        tag: Tag {
            owner: browser.owner,
            serial: 0,
            connection: browser.session.generation,
            query: Query::Mine,
        },
        rx,
        _scope: Default::default(),
    });
    browser.poll();
    assert!(browser.pending.is_none());
    assert_eq!(browser.error, Some(DiscoveryError::Transport));
}

#[test]
fn browser_localized_modal_keeps_keyboard_close_accessible() {
    use egui_kittest::{Harness, kittest::Queryable};
    crate::i18n::set_language("en-US");
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (Browser, bool)| {
            state.1 |= state.0.show(ui.ctx()).0;
        },
        (browser(), false),
    );
    harness.run();
    harness.get_by_label("Browse GitHub");
    harness.get_by_label("Clone a URL");
    crate::i18n::set_language("de");
    harness.step();
    harness.get_by_label("GitHub durchsuchen");
    harness.get_by_label("URL klonen");
    harness.get_by_label("Schließen").focus();
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(harness.state().1);
    crate::i18n::set_language("en-US");
}

#[test]
fn browser_invalid_session_clears_private_results_and_cancels_pending() {
    let mut browser = browser();
    browser.loaded = true;
    browser.session.generation = u64::MAX;
    browser.poll();
    assert!(!browser.loaded);
    assert!(browser.query.is_none());
    assert!(browser.pending.is_none());
}

fn repository() -> Repository {
    Repository {
        id: 1,
        hosting: crate::forge::Hosting {
            forge: crate::forge::Forge::GitHub,
            host: "github.com".into(),
            path: "owner/repo".into(),
            web: "https://github.com".into(),
        },
        owner: "owner".into(),
        name: "repo".into(),
        description: "A repository".into(),
        language: Some("Rust".into()),
        stars: Some(3),
        visibility: crate::forge::discovery::Visibility::Private,
        web_url: "https://github.com/owner/repo".into(),
        https_url: "https://github.com/owner/repo.git".into(),
        ssh_url: "git@github.com:owner/repo.git".into(),
    }
}
#[test]
fn browser_deduplicates_and_preserves_loaded_rows_when_next_page_fails() {
    let mut browser = browser();
    let mut first = page(Some("page2"));
    first.repositories.push(repository());
    browser.apply(Ok(first));
    browser.apply(Err(DiscoveryError::Transport));
    assert_eq!(browser.repositories.len(), 1);
    assert_eq!(browser.next.as_deref(), Some("page2"));
    let mut repeated = page(Some("page3"));
    repeated.repositories.push(repository());
    browser.apply(Ok(repeated));
    assert_eq!(browser.repositories.len(), 1);
    assert!(browser.next.is_none());
    browser.session.generation = u64::MAX;
    browser.poll();
    assert!(browser.repositories.is_empty());
}

#[test]
fn browser_discards_reply_for_another_window() {
    let mut browser = browser();
    browser.query = Some(Query::Mine);
    let expected = Tag {
        owner: browser.owner,
        serial: 0,
        connection: browser.session.generation,
        query: Query::Mine,
    };
    let mut wrong = expected.clone();
    wrong.owner = egui::ViewportId::from_hash_of("other-window");
    let (tx, rx) = std::sync::mpsc::channel();
    let mut reply = page(None);
    reply.repositories.push(repository());
    assert!(tx.send((wrong, Ok(reply))).is_ok());
    browser.pending = Some(Pending {
        tag: expected,
        rx,
        _scope: Default::default(),
    });
    browser.poll();
    assert!(browser.pending.is_none());
    assert!(browser.repositories.is_empty());
}

#[test]
fn browser_clone_handoff_contains_both_transports_without_credentials() {
    use egui_kittest::{Harness, kittest::Queryable};
    crate::i18n::set_language("en-US");
    let mut browser = browser();
    browser.repositories.push(repository());
    browser.loaded = true;
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (Browser, Option<Intent>)| {
            let (_, intent) = state.0.show(ui.ctx());
            if intent.is_some() {
                state.1 = intent;
            }
        },
        (browser, None),
    );
    harness.run();
    harness.get_by_label("Clone…").click();
    harness.run();
    assert!(
        matches!(&harness.state().1, Some(Intent::Clone { https, ssh }) if https == "https://github.com/owner/repo.git" && ssh == "git@github.com:owner/repo.git")
    );
}
