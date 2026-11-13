//! The compact selected commit (#213), drawn: no files switch in its header, a long
//! description folded behind _Show more_, a short one whole, the four sections folded into
//! _More details_ with a signature warning still in sight, at narrow and wide sizes.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::git::{Signature, SignatureFormat, SignatureVerdict};
use crate::repo::{Action, RepoTab};

const LONG: &str = "feat: a commit with a long description\n\nFirst paragraph of the \
description, which says what changed.\n\nSecond paragraph.\n\nThird paragraph.\n\nFourth \
paragraph.\n\nFifth paragraph.\n\nCo-authored-by: Someone <someone@example.invalid>\n\nThe very \
last line of the description.";

fn tab(message: &str, verdict: SignatureVerdict) -> RepoTab {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/compact-detail".into(), &ctx);
    let hash = "a".repeat(40);
    tab.refs.head = hash.clone();
    tab.detail = Some(crate::git::CommitDetail {
        hash: hash.clone(),
        parents: vec!["b".repeat(40)],
        author: "An author".into(),
        email: "author@example.invalid".into(),
        author_date: "2026-11-12 14:16:49 +0100".into(),
        committer: "An author".into(),
        commit_date: "2026-11-12 14:16:49 +0100".into(),
        message: message.into(),
        files: vec![crate::git::ChangedFile {
            path: "src/repo/file.rs".into(),
            old_path: None,
            status: 'M',
            added: Some(3),
            removed: Some(1),
        }],
        in_head: false,
        merges_after: false,
    });
    tab.detail_hash = Some(hash.clone());
    tab.extras.hash = Some(hash);
    tab.extras.signature = Some(Ok(Signature {
        verdict,
        format: Some(SignatureFormat::Gpg),
        signer: String::new(),
        key: "ABCDEF".into(),
        primary_key: String::new(),
        raw: String::new(),
    }));
    tab.extras.contained = Some(Err("not read here".into()));
    tab.extras.notes.note = Some(Ok(None));
    tab.extras.backlinks = Some(Ok((Vec::new(), false)));
    tab
}

fn harness(width: f32, tab: RepoTab) -> Harness<'static, (RepoTab, Vec<Action>)> {
    let mut styled = false;
    let mut h = Harness::builder().with_size([width, 900.0]).build_ui_state(
        move |ui, (tab, acts): &mut (RepoTab, Vec<Action>)| {
            if !styled {
                crate::theme::install_fonts(ui.ctx());
                crate::theme::apply(
                    ui.ctx(),
                    &crate::theme::ThemeId::default(),
                    &crate::theme::Tweaks::default(),
                );
                egui_extras::install_image_loaders(ui.ctx());
                styled = true;
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| tab.commit_panel(ui, acts));
        },
        (tab, Vec::new()),
    );
    for _ in 0..5 {
        h.step();
    }
    h
}

fn expanded<T>(h: &Harness<'_, T>, label: &str) -> Option<bool> {
    h.get_by_label(label).accesskit_node().data().is_expanded()
}

#[test]
fn a_long_description_folds_behind_show_more_and_comes_back_whole() {
    for width in [260.0, 420.0, 760.0] {
        let mut h = harness(width, tab(LONG, SignatureVerdict::Valid { trusted: true }));
        assert_eq!(expanded(&h, "Show more"), Some(false), "at {width}");
        // Folded, the description is cut short on screen (its whole text stays readable to
        // screen readers and on hover).
        let folded = h.get_by_label_contains("First paragraph").rect().height();
        // The files switch is gone from the header; the file list is right there.
        assert!(h.query_by_label("Tree").is_none() && h.query_by_label("Path").is_none());
        assert!(h.query_by_label_contains("file.rs").is_some());
        crate::repo::detail::responsive_tests::evidence(&mut h, &format!("compact-folded-{width}"));
        h.get_by_label("Show more").click();
        h.step();
        h.step();
        assert_eq!(expanded(&h, "Show less"), Some(true));
        let unfolded = h.get_by_label_contains("First paragraph").rect().height();
        assert!(unfolded > folded * 2.0, "{folded} → {unfolded} at {width}");
        crate::repo::detail::responsive_tests::evidence(
            &mut h,
            &format!("compact-unfolded-{width}"),
        );
        // The choice stays from commit to commit: no jump back.
        h.get_by_label("Show less").click();
        h.step();
        h.step();
        assert!(h.query_by_label("Show more").is_some());
    }
}

#[test]
fn a_short_description_shows_whole_without_an_expander() {
    let h = harness(
        420.0,
        tab("fix: short\n\nOne line.", SignatureVerdict::Unsigned),
    );
    assert!(h.query_by_label_contains("One line.").is_some());
    assert!(h.query_by_label("Show more").is_none());
}

#[test]
fn the_sections_fold_into_more_details_and_a_warning_stays_visible() {
    for width in [240.0, 760.0] {
        let mut h = harness(width, tab("fix: signed", SignatureVerdict::UnknownKey));
        assert_eq!(expanded(&h, "More details"), Some(false), "at {width}");
        for title in ["Signature", "Contained in", "Notes", "Backlinks"] {
            assert!(
                h.query_by_label(title).is_none(),
                "{title} shown while folded"
            );
        }
        // The signature's warning is in sight, and opens the details at the signature.
        let warning = "Signature: Unknown key · GPG";
        let button = egui::accesskit::Role::Button;
        h.get_by_role_and_label(button, warning).click();
        h.step();
        h.step();
        assert_eq!(expanded(&h, "More details"), Some(true));
        assert_eq!(expanded(&h, "Signature"), Some(true));
        for title in ["Contained in", "Notes", "Backlinks"] {
            assert!(
                h.query_by_label(title).is_some(),
                "{title} missing when open"
            );
        }
        crate::repo::detail::responsive_tests::evidence(&mut h, &format!("compact-more-{width}"));
        h.get_by_label("More details").click();
        h.step();
        assert!(h.query_by_label("Backlinks").is_none());
    }
}

#[test]
fn a_good_or_missing_signature_raises_no_warning() {
    for verdict in [
        SignatureVerdict::Valid { trusted: true },
        SignatureVerdict::Unsigned,
    ] {
        let h = harness(420.0, tab("fix: fine", verdict));
        assert!(h.query_by_label_contains("Signature:").is_none());
        assert!(h.query_by_label("More details").is_some());
    }
    assert!(super::needs_attention(&SignatureVerdict::Revoked));
    assert!(super::needs_attention(&SignatureVerdict::Expired {
        key: true
    }));
}
