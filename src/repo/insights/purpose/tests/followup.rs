//! Follow-up findings from the independent review of de884625.
use super::*;
use crate::repo::{Msg, RepoTab, messages};

#[test]
fn rejected_proposal_completes_with_error_and_can_retry() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording-only/purpose-rejection".into(), &ctx);
    tab.review_purpose.units = Some(Ok(std::sync::Arc::new(mixed())));
    let (_, mut jobs) = messages::record_rejected_jobs(|| tab.purpose_refresh(&ctx));
    assert!(tab.review_purpose.arranged.is_none());
    let Msg::Then(complete) = jobs.pop().ok_or("missing rejected job")?() else {
        return Err("rejection did not use the feature completion".into());
    };
    complete(&mut tab, &ctx);
    assert!(
        matches!(&tab.review_purpose.arranged, Some(Err(error)) if error == "recorded queue rejection")
    );
    let (_, mut jobs) = messages::record_jobs(|| tab.purpose_refresh(&ctx));
    let Msg::Then(complete) = jobs.pop().ok_or("missing retry")?() else {
        return Err("retry did not complete".into());
    };
    complete(&mut tab, &ctx);
    assert!(matches!(tab.review_purpose.arranged, Some(Ok(_))));
    Ok(())
}

#[test]
fn stale_rejected_proposals_cannot_replace_a_new_request_or_proposal() -> Result<(), String> {
    for new_request in [true, false] {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/recording-only/purpose-stale-rejection".into(), &ctx);
        tab.review_purpose.units = Some(Ok(std::sync::Arc::new(mixed())));
        let (_, mut jobs) = messages::record_rejected_jobs(|| tab.purpose_refresh(&ctx));
        if new_request {
            tab.review_purpose.request += 1;
        } else {
            tab.review_purpose.proposing += 1;
        }
        let Msg::Then(complete) = jobs.pop().ok_or("missing rejection")?() else {
            return Err("rejection did not complete".into());
        };
        complete(&mut tab, &ctx);
        assert!(tab.review_purpose.arranged.is_none());
    }
    Ok(())
}

#[test]
fn rename_editor_controls_translate_and_cancel_or_rename_the_original_group() -> Result<(), String>
{
    use egui_kittest::{Harness, kittest::Queryable};
    for (language, rename_label, cancel_label) in [
        ("en-US", "Rename", "Cancel"),
        ("de", "Umbenennen", "Abbrechen"),
    ] {
        crate::i18n::set_language(language);
        let u = mixed();
        let proposal = propose(&u, &lockfiles);
        let arranged = arrange::arrange(&proposal, &u, None);
        let group = arranged.groups.first().ok_or("missing group")?.clone();
        let id = group.group.id.clone();
        let files = crate::pull_request::PrFiles {
            from: "base".into(),
            to: "head".into(),
            files: vec![],
        };
        let selected = Default::default();
        let fonts = std::cell::Cell::new(false);
        let edit_id = egui::Id::new(("purpose-rename", &id));
        let mut h = Harness::new_ui_state(
            move |ui, acts: &mut Vec<crate::repo::Action>| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    ui.data_mut(|d| d.insert_temp(edit_id, "Renamed 日本語".to_owned()));
                    return;
                }
                rows::group(
                    ui,
                    &rows::Ctx {
                        units: &u,
                        arranged: &arranged,
                        files: &files,
                        threads: &[],
                        selected: &selected,
                        open: None,
                    },
                    0,
                    &group,
                    acts,
                );
            },
            Vec::new(),
        );
        h.run();
        h.get_by_label(rename_label);
        h.get_by_label(cancel_label).click();
        h.run();
        assert!(h.state().is_empty());
        h.ctx
            .data_mut(|d| d.insert_temp(edit_id, "Renamed 日本語".to_owned()));
        h.run();
        h.get_by_label(rename_label).click();
        h.run();
        assert!(h.state().iter().any(|act| matches!(act,
            crate::repo::Action::Insights(crate::repo::insights::InsightsAct::Purpose(super::super::Act::Edit(Edit::Rename { group, title })))
            if group == &id && title == "Renamed 日本語")));
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
