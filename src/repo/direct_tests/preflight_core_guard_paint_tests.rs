use super::*;
use egui::{Context, Event, FullOutput, Pos2, RawInput, Rect, Vec2, epaint::Shape};

// Inert egui frames only. No App, RepoTab, readers, discovery or workers.
pub(in crate::repo) fn frame(
    ctx: &Context,
    events: Vec<Event>,
    draw: &mut impl FnMut(&mut egui::Ui),
) -> FullOutput {
    let initialized = ctx.data_mut(|data| {
        let id = egui::Id::new("core-guard-inert-fonts");
        let initialized = data.get_temp::<bool>(id).unwrap_or(false);
        data.insert_temp(id, true);
        initialized
    });
    if !initialized {
        // Embedded font/image bytes only; no native renderer or theme discovery.
        crate::theme::install_fonts(ctx);
        egui_extras::install_image_loaders(ctx);
    }
    ctx.all_styles_mut(|style| {
        style.animation_time = 0.0;
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });
    // Pointer press/release must remain inside egui's ordinary click interval.
    let time = ctx.input(|i| i.time) + 0.1;
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1800., 4000.))),
            time: Some(time),
            events,
            ..Default::default()
        },
        draw,
    );
    output.textures_delta.clear();
    output
}

pub(in crate::repo) fn text(output: &FullOutput) -> String {
    fn collect(shape: &Shape, out: &mut Vec<String>) {
        match shape {
            Shape::Text(t) => out.push(t.galley.text().to_owned()),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut out);
    }
    out.join("\n")
}

pub(in crate::repo) fn paint(ctx: &Context, mut draw: impl FnMut(&mut egui::Ui)) -> String {
    let _ = frame(ctx, vec![], &mut draw);
    text(&frame(ctx, vec![], &mut draw))
}

pub(in crate::repo) fn click(
    ctx: &Context,
    label: &str,
    mut draw: impl FnMut(&mut egui::Ui),
) -> Result<(), String> {
    let _ = frame(ctx, vec![], &mut draw);
    let output = frame(ctx, vec![], &mut draw);
    let position = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() / 2.),
            _ => None,
        })
        .ok_or_else(|| format!("Missing button {label}: {}", text(&output)))?;
    for pressed in [true, false] {
        let _ = frame(
            ctx,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut draw,
        );
    }

    Ok(())
}

#[test]
fn core_guard_paint_checklist_keeps_supplied_plan_and_reload_action_raw() -> Result<(), String> {
    use crate::git::repository_tools::routes::PushRoute;
    let route = PushRoute {
        branch: "topic-12345".into(),
        commit: "a".repeat(40),
        remote: "remote-12345".into(),
        destination: "refs/heads/release-12345".into(),
        urls: vec![
            "[sensitive path]".into(),
            "example.invalid:[sensitive path]".into(),
        ],
        lease: Some("b".repeat(40)),
    };
    let mut plan = check::selected_continuation_recording::plan(&route);
    plan.expected_name = "Author 12345".into();
    plan.expected_email = "author12345@example.invalid".into();
    plan.lfs_objects.insert("c".repeat(64));
    plan.findings.push(check::Finding {
        kind: Kind::Identity,
        commit: route.commit.clone(),
        path: "src/12345\nopaque.rs".into(),
        detail:
            "Author email differs from the profile policy for Local repository: [sensitive path]."
                .into(),
    });
    plan.findings.push(check::Finding {
        kind: Kind::Secret,
        commit: route.commit.clone(),
        path: "src/12345.rs".into(),
        detail: "Likely secret at line 12345; value withheld.".into(),
    });
    let original_ops = plan.again();
    let original_args = check::push_args(&plan).map_err(|error| format!("{error:?}"))?;
    let targets = plan.targets.clone();
    let findings = plan
        .findings
        .iter()
        .map(|f| (f.kind, f.commit.clone(), f.path.clone(), f.detail.clone()))
        .collect::<Vec<_>>();
    let ctx = Context::default();
    for (locale, caption, number) in [
        ("en-US", "Local repository: [sensitive path]", "12,345"),
        ("de", "Lokales Repository: [sensitive path]", "12.345"),
    ] {
        crate::i18n::set_language(locale);
        let mut acknowledge = false;
        let shown = paint(&ctx, |ui| {
            assert!(matches!(
                dialog(ui, &mut plan, &mut acknowledge),
                Outcome::Open
            ));
        });
        for opaque in [
            "refs/heads/topic-12345",
            "refs/heads/release-12345",
            "Author 12345",
            "author12345@example.invalid",
            "example.invalid/[sensitive path]",
            "src/12345\\nopaque.rs",
        ] {
            assert!(shown.contains(opaque), "missing {opaque}: {shown}");
        }
        assert!(shown.contains(caption), "{shown}");
        assert!(shown.contains(number), "{shown}");
        let mut outcome = Outcome::Open;
        click(&ctx, crate::i18n::text("repo-core-3171"), |ui| {
            outcome = dialog(ui, &mut plan, &mut acknowledge);
        })?;
        let Outcome::Done(action) = outcome else {
            return Err("reload action missing".into());
        };
        let Action::ReloadPreflight(reload) = *action else {
            return Err("unexpected action".into());
        };
        assert_eq!(reload.again(), original_ops);
        assert_eq!(
            check::push_args(&reload).map_err(|error| format!("{error:?}"))?,
            original_args
        );
        assert_eq!(reload.targets, targets);
        assert_eq!(plan.again(), original_ops);
        assert_eq!(
            check::push_args(&plan).map_err(|error| format!("{error:?}"))?,
            original_args
        );
        assert_eq!(
            plan.findings
                .iter()
                .map(|f| (f.kind, f.commit.clone(), f.path.clone(), f.detail.clone()))
                .collect::<Vec<_>>(),
            findings
        );
        assert!(!acknowledge);
    }
    crate::i18n::set_language("en-US");

    Ok(())
}
