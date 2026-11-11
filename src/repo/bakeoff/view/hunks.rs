use super::*;

pub(super) fn clashes(sides: &[Side], picked: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let span = |(n, i): (usize, usize)| {
        let h = sides.get(n)?.hunks.get(i)?;
        let start = if h.removed.is_empty() {
            h.old_start
        } else {
            h.old_start - 1
        };
        Some((start, start + h.removed.len(), h.removed.is_empty()))
    };
    let mut out = Vec::new();
    for (a, &x) in picked.iter().enumerate() {
        for &y in &picked[a + 1..] {
            if x.0 == y.0 {
                continue;
            }
            if let (Some((s1, e1, ins1)), Some((s2, e2, ins2))) = (span(x), span(y)) {
                // Two that replace lines overlap when their ranges do; an insertion clashes
                // with a replacement whose range has it inside.
                let overlap = match (ins1, ins2) {
                    (true, true) => false,
                    (true, false) => s1 > s2 && s1 < e2,
                    (false, true) => s2 > s1 && s2 < e1,
                    (false, false) => s1 < e2 && s2 < e1,
                };
                if overlap {
                    out.extend([x, y]);
                }
            }
        }
    }
    out
}

/// The file shown in a column per attempt that changed it: its patch, the button that takes
/// it whole and its hunks to take one by one.
pub(super) fn side_by_side(
    ui: &mut Ui,
    view: &Bakeoff,
    path: &str,
    sides: Option<&Result<Vec<Side>, String>>,
    t: &Tokens,
    act: &mut Option<BakeAct>,
) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(RichText::new(path).font(theme::semibold(13.0)));
        if view.picks.contains_key(path)
            && ui
                .small_button(crate::i18n::text(
                    "repo-workflows-bakeoff-side-by-side-6bdfa343",
                ))
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-bakeoff-side-by-side-76215eb4",
                ))
                .clicked()
        {
            *act = Some(BakeAct::ClearPick(path.to_owned()));
        }
    });
    let sides = match sides {
        None => {
            ui.add(crate::spinner::Spinner::new());
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(t.danger, e);
            return;
        }
        Some(Ok(sides)) => sides,
    };
    let take = view.picks.get(path);
    let picked: Vec<(usize, usize)> = match take {
        Some(Take::Hunks(list)) => list.clone(),
        _ => Vec::new(),
    };
    let clashing = clashes(sides, &picked);
    if !clashing.is_empty() {
        ui.colored_label(
            t.danger,
            crate::i18n::text("repo-workflows-bakeoff-side-by-side-f280944f"),
        );
    }
    let changed: Vec<usize> = (0..sides.len())
        .filter(|n| sides[*n].kind() != "unchanged")
        .collect();
    let unchanged: Vec<&str> = (0..sides.len())
        .filter(|n| !changed.contains(n))
        .filter_map(|n| view.attempts.get(n).map(|a| a.name.as_str()))
        .collect();
    if !unchanged.is_empty() {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-bakeoff-side-by-side-47c58b04",
                &[("unchanged-join-1", (unchanged.join(", ")).clone().into())],
            ))
            .small()
            .color(t.text_faint),
        );
    }
    ui.columns(changed.len().max(1), |cols| {
        for (col, &n) in cols.iter_mut().zip(&changed) {
            let side = &sides[n];
            let name = view.attempts.get(n).map_or("?", |a| a.name.as_str());
            column(col, side, name, n, path, take, &clashing, t, act);
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn column(
    ui: &mut Ui,
    side: &Side,
    name: &str,
    n: usize,
    path: &str,
    take: Option<&Take>,
    clashing: &[(usize, usize)],
    t: &Tokens,
    act: &mut Option<BakeAct>,
) {
    ui.label(
        RichText::new(format!("{name} · {}", side.display_kind())).font(theme::semibold(12.0)),
    );
    let whole = take == Some(&Take::File(n));
    let label = match side.kind() {
        "deleted" => crate::i18n::text("repo-workflows-bakeoff-column-772841f1"),
        "added" => crate::i18n::text("repo-workflows-bakeoff-column-badecf50"),
        _ => crate::i18n::text("repo-workflows-bakeoff-column-a533487d"),
    };
    if ui
        .selectable_label(whole, label)
        .on_hover_text(crate::i18n::text("repo-workflows-bakeoff-column-6e86718f"))
        .clicked()
    {
        *act = Some(BakeAct::TakeFile(path.to_owned(), n));
    }
    egui::ScrollArea::vertical()
        .id_salt(("bakeoff-hunks", n))
        .max_height(140.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if side.hunks.is_empty() {
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-bakeoff-column-2ed1ade8"))
                        .small()
                        .color(t.text_faint),
                );
            }
            for (i, hunk) in side.hunks.iter().enumerate() {
                let on = matches!(take, Some(Take::Hunks(list)) if list.contains(&(n, i)));
                let mut checked = on;
                let text = RichText::new(hunk_label(hunk)).font(theme::mono(11.0));
                let text = if clashing.contains(&(n, i)) {
                    text.color(t.danger)
                } else {
                    text
                };
                if ui
                    .checkbox(&mut checked, text)
                    .on_hover_text(hunk_preview(hunk))
                    .changed()
                {
                    *act = Some(BakeAct::ToggleHunk(path.to_owned(), n, i));
                }
            }
        });
    ui.separator();
    let lines: Vec<&str> = side
        .patch
        .lines()
        .skip_while(|l| !l.starts_with("@@"))
        .take(MAX_PATCH_LINES)
        .collect();
    // Only the lines in sight are laid out: a patch can be thousands of lines.
    egui::ScrollArea::both()
        .id_salt(("bakeoff-side", n))
        .auto_shrink([false, false])
        .show_rows(ui, ROW_H, lines.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for line in &lines[range] {
                patch_line(ui, line, t);
            }
        });
}

/// The first lines a hunk removes and adds, for its tooltip.
pub(super) fn hunk_preview(hunk: &Hunk) -> String {
    let lines: Vec<String> = hunk
        .removed
        .iter()
        .map(|l| format!("- {l}"))
        .chain(hunk.added.iter().map(|l| format!("+ {l}")))
        .take(12)
        .collect();
    lines.join("\n")
}

pub(super) fn patch_line(ui: &mut Ui, line: &str, t: &Tokens) {
    let (color, fill) = match line.chars().next() {
        Some('+') => (t.text, t.diff_add_bg),
        Some('-') => (t.text, t.diff_del_bg),
        Some('@') => (t.text_faint, egui::Color32::TRANSPARENT),
        _ => (t.text_dim, egui::Color32::TRANSPARENT),
    };
    egui::Frame::new().fill(fill).show(ui, |ui| {
        ui.set_min_height(ROW_H);
        ui.label(RichText::new(line).font(theme::mono(11.5)).color(color));
    });
}
