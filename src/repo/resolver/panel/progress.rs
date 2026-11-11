use super::*;

pub(super) fn step_color(t: &Tokens, status: StepStatus) -> Color32 {
    match status {
        StepStatus::Pending => t.field,
        StepStatus::Running => t.accent,
        StepStatus::Done => t.success,
        StepStatus::Failed => t.danger,
        StepStatus::Skipped => t.text_faint,
    }
}

/// The plan as a bar: a segment per step, coloured by how it stands.
pub(super) fn step_bar(ui: &mut Ui, t: &Tokens, run: &Run) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let steps = &run.progress.steps;
    if steps.is_empty() {
        painter.rect_filled(rect, 3.0, t.field);
        return;
    }
    let gap = 3.0;
    let n = steps.len() as f32;
    let each = ((rect.width() - gap * (n - 1.0)) / n).max(2.0);
    for (i, step) in steps.iter().enumerate() {
        let x = rect.left() + i as f32 * (each + gap);
        let seg = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(each, rect.height()));
        painter.rect_filled(seg, 3.0, step_color(t, step.status));
        if step.status == StepStatus::Running {
            // Part way: a lighter band over the second half.
            let rest = egui::Rect::from_min_max(seg.center_top(), seg.right_bottom());
            painter.rect_filled(rest, 3.0, t.accent.gamma_multiply(0.35));
        }
    }
}

pub(super) fn step_icon(ui: &mut Ui, t: &Tokens, status: StepStatus) {
    let size = vec2(16.0, 16.0);
    match status {
        StepStatus::Running => {
            ui.add(crate::spinner::Spinner::new().size(14.0).color(t.accent));
        }
        StepStatus::Done => icons::inline(ui, "check", 15.0, t.success),
        StepStatus::Failed => icons::inline(ui, "x", 15.0, t.danger),
        StepStatus::Skipped => icons::inline(ui, "minus", 15.0, t.text_faint),
        StepStatus::Pending => {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.painter()
                .circle_stroke(rect.center(), 5.0, Stroke::new(1.5, t.text_faint));
        }
    }
}

pub(super) fn steps(ui: &mut Ui, t: &Tokens, steps: &[Step], height: f32) {
    egui::ScrollArea::vertical()
        .id_salt("resolver-steps")
        .max_height(height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for step in steps {
                ui.horizontal_top(|ui| {
                    step_icon(ui, t, step.status);
                    ui.vertical(|ui| {
                        let color = match step.status {
                            StepStatus::Pending | StepStatus::Skipped => t.text_dim,
                            _ => t.text,
                        };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&step.title).color(color));
                            if let (Some(a), Some(b)) = (step.started, step.ended) {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let took = b.saturating_duration_since(a);
                                        ui.label(
                                            RichText::new(clock(took)).color(t.text_faint).small(),
                                        );
                                    },
                                );
                            }
                        });
                        if let Some(file) = step.file.as_deref().filter(|f| *f != step.title) {
                            ui.label(
                                RichText::new(file)
                                    .font(theme::mono(11.0))
                                    .color(t.text_faint),
                            );
                        }
                        if let Some(note) = &step.note {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(note).color(t.text_dim).small().italics(),
                                )
                                .wrap(),
                            );
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });
}

/// The last few things the agent did, the newest brightest.
pub(super) fn recent(ui: &mut Ui, t: &Tokens, run: &Run) {
    let Some(agent) = &run.agent else { return };
    let lines: Vec<String> = agent.with_feed(|f| {
        f.activities
            .iter()
            .rev()
            .filter_map(|a| match a {
                Activity::Tool(s) => Some(format!("› {s}")),
                Activity::Said(s) => Some(s.clone()),
                Activity::Ended { .. } => None,
            })
            .take(RECENT)
            .collect()
    });
    if lines.is_empty() {
        return;
    }
    ui.add_space(6.0);
    egui::Frame::new()
        .fill(t.field)
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (i, line) in lines.iter().enumerate().rev() {
                let color = if i == 0 { t.text_dim } else { t.text_faint };
                ui.add(
                    egui::Label::new(RichText::new(line).font(theme::mono(11.0)).color(color))
                        .truncate(),
                );
            }
        });
}
