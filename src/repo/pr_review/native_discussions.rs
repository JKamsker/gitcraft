//! Native discussions remain reachable independently of the currently displayed diff.
use super::{PrAct, PrDraft, pr_acts};
use crate::{pr_threads::PrExtras, repo::Action};
pub(super) fn show(
    ui: &mut egui::Ui,
    draft: &mut PrDraft,
    extras: &PrExtras,
    acts: &mut Vec<Action>,
) {
    if !extras.native_policy {
        return;
    }
    for thread in &extras.threads {
        let coordinate = if thread.path.is_empty() {
            String::new()
        } else {
            format!(
                " · {}:{} ({})",
                thread.path,
                thread
                    .line
                    .map_or_else(|| "unmapped".into(), |n| n.to_string()),
                if thread.side == crate::pull_request::DiffSide::Left {
                    "old side"
                } else {
                    "new side"
                }
            )
        };
        egui::CollapsingHeader::new(format!(
            "Discussion · {}{}{}{}",
            thread.comments.first().map_or("", |c| c.author.as_str()),
            coordinate,
            if thread.outdated {
                " · outdated position"
            } else {
                ""
            },
            if thread.resolved { " · resolved" } else { "" }
        ))
        .id_salt(("native-discussion", &thread.id))
        .default_open(thread.outdated)
        .show(ui, |ui| {
            for comment in &thread.comments {
                ui.label(egui::RichText::new(&comment.author).strong());
                ui.add(egui::Label::new(&comment.body).selectable(true));
            }
            if thread.can_resolve
                && ui
                    .add_enabled(!draft.posting, egui::Button::new("Resolve discussion"))
                    .clicked()
            {
                pr_acts(acts, PrAct::SetResolved(thread.id.clone(), true));
            }
            if thread.can_unresolve
                && ui
                    .add_enabled(!draft.posting, egui::Button::new("Reopen discussion"))
                    .clicked()
            {
                pr_acts(acts, PrAct::SetResolved(thread.id.clone(), false));
            }
            if thread.can_reply {
                ui.add_enabled(
                    !draft.posting,
                    egui::TextEdit::multiline(&mut draft.comment)
                        .hint_text("Reply to this discussion")
                        .desired_rows(2),
                );
                if ui
                    .add_enabled(
                        !draft.posting && !draft.comment.trim().is_empty(),
                        egui::Button::new("Reply to discussion"),
                    )
                    .clicked()
                {
                    pr_acts(acts, PrAct::ReplyConversation(thread.id.clone()));
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pr_threads::{Thread, ThreadComment};
    fn fixture(resolved: bool) -> PrExtras {
        PrExtras {
            native_policy: true,
            threads: vec![Thread {
                id: "old-thread".into(),
                path: "removed.rs".into(),
                line: Some(42),
                start_line: None,
                side: crate::pull_request::DiffSide::Left,
                outdated: true,
                resolved,
                can_resolve: !resolved,
                can_unresolve: resolved,
                can_reply: true,
                comments: vec![ThreadComment {
                    author: "reviewer".into(),
                    body: "Original discussion".into(),
                    ..Default::default()
                }],
                starts_left: false,
                more_comments: false,
            }],
            ..Default::default()
        }
    }
    fn render(
        ctx: &egui::Context,
        draft: &mut PrDraft,
        extras: &PrExtras,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<Action>) {
        let mut acts = vec![];
        let out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| show(ui, draft, extras, &mut acts),
        );
        (out, acts)
    }
    fn text(out: &egui::FullOutput) -> String {
        out.shapes
            .iter()
            .filter_map(|s| {
                if let egui::Shape::Text(t) = &s.shape {
                    Some(t.galley.text())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn click(
        ctx: &egui::Context,
        draft: &mut PrDraft,
        extras: &PrExtras,
        label: &str,
    ) -> Result<Vec<Action>, String> {
        let (out, _) = render(ctx, draft, extras, vec![]);
        let p = out
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Text(t) = &s.shape {
                    (t.galley.text() == label).then_some(t.pos + t.galley.size() * 0.5)
                } else {
                    None
                }
            })
            .ok_or("required native discussion button")?;
        out.drop_without_applying_deltas();
        let event = |pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        render(
            ctx,
            draft,
            extras,
            vec![egui::Event::PointerMoved(p), event(true)],
        )
        .0
        .drop_without_applying_deltas();
        let (out, acts) = render(ctx, draft, extras, vec![event(false)]);
        out.drop_without_applying_deltas();
        Ok(acts)
    }
    #[test]
    fn outdated_file_discussion_controls_are_visible_and_route_original_thread()
    -> Result<(), String> {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.all_styles_mut(|s| s.animation_time = 0.0);
        let extras = fixture(false);
        let mut draft = PrDraft {
            comment: "Reply".into(),
            ..Default::default()
        };
        let (out, _) = render(&ctx, &mut draft, &extras, vec![]);
        let shown = text(&out);
        assert!(shown.contains("removed.rs:42 (old side)"));
        assert!(shown.contains("outdated position"));
        assert!(shown.contains("Original discussion"));
        assert!(shown.contains("Resolve discussion"));
        assert!(shown.contains("Reply to discussion"));
        out.drop_without_applying_deltas();
        assert!(
            click(&ctx, &mut draft, &extras, "Resolve discussion")?
                .iter()
                .any(|a| matches!(a,Action::Pr(PrAct::SetResolved(id,true)) if id=="old-thread"))
        );
        assert!(
            click(&ctx, &mut draft, &extras, "Reply to discussion")?
                .iter()
                .any(|a| matches!(a,Action::Pr(PrAct::ReplyConversation(id)) if id=="old-thread"))
        );
        assert!(
            click(&ctx, &mut draft, &fixture(true), "Reopen discussion")?
                .iter()
                .any(|a| matches!(a,Action::Pr(PrAct::SetResolved(id,false)) if id=="old-thread"))
        );
        Ok(())
    }
}
