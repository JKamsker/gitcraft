//! Absorb target descriptions and destination picker.

use super::*;

/// What a match says, in which colour.
pub(super) fn verdict(
    t: &Tokens,
    editor: &AbsorbEditor,
    found: Match,
    touched: &[usize],
) -> (String, Color32) {
    let name = |i: usize| {
        editor
            .targets
            .get(i)
            .map(|c| short(&c.hash).to_owned())
            .unwrap_or_default()
    };
    match found {
        Match::Clear(i) => (
            crate::i18n::message(
                "repo-workflows-surgery-verdict-b5ced75e",
                &[("name-i-1", (name(i)).clone().into())],
            ),
            t.success,
        ),
        Match::Several(i) => (
            crate::i18n::message(
                "repo-workflows-surgery-verdict-e6cd9adf",
                &[
                    (
                        "touched-iter-map-i-name-i-collec-1",
                        (touched
                            .iter()
                            .map(|&i| name(i))
                            .collect::<Vec<_>>()
                            .join(", "))
                        .clone()
                        .into(),
                    ),
                    ("name-i-2", (name(i)).clone().into()),
                ],
            ),
            t.warning,
        ),
        Match::None => (
            crate::i18n::text("repo-workflows-surgery-verdict-4f2f85d1").to_owned(),
            t.text_faint,
        ),
    }
}

/// A commit as the picker lists it: ● when it changed the lines, ⇡ when it is pushed.
pub(super) fn target_text(
    editor: &AbsorbEditor,
    i: usize,
    touched: bool,
    suggested: bool,
) -> String {
    let Some(c) = editor.targets.get(i) else {
        return String::new();
    };
    format!(
        "{}{}  {}{}{}",
        if touched { "● " } else { "   " },
        short(&c.hash),
        c.summary,
        if c.pushed {
            crate::i18n::text("repo-workflows-surgery-target-text-252d53a4")
        } else {
            ""
        },
        if suggested {
            crate::i18n::text("repo-workflows-surgery-target-text-ea3d92f5")
        } else {
            ""
        },
    )
}

/// The picker of where hunk `h` goes: those that changed its lines first.
pub(super) fn picker(
    ui: &mut Ui,
    editor: &AbsorbEditor,
    h: usize,
    chosen: &mut Option<(usize, Option<usize>)>,
) {
    let Some(hunk) = editor.hunks.get(h) else {
        return;
    };
    let choice = editor.choice.get(h).copied().flatten();
    let suggested = match hunk.found {
        Match::Several(i) | Match::Clear(i) => Some(i),
        Match::None => None,
    };
    let shown = match choice {
        Some(c) => target_text(editor, c, hunk.touched.contains(&c), suggested == Some(c)),
        None => crate::i18n::text("repo-workflows-surgery-picker-e45ce620").to_owned(),
    };
    egui::ComboBox::from_id_salt("into")
        .width(320.0)
        .truncate()
        .selected_text(shown.trim_start())
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(
                    choice.is_none(),
                    crate::i18n::text("repo-workflows-surgery-picker-e45ce620"),
                )
                .clicked()
            {
                *chosen = Some((h, None));
            }
            let others = (0..editor.targets.len()).filter(|i| !hunk.touched.contains(i));
            for (n, i) in hunk.touched.iter().copied().chain(others).enumerate() {
                if n == hunk.touched.len() && n > 0 {
                    ui.separator();
                }
                let text = target_text(editor, i, hunk.touched.contains(&i), suggested == Some(i));
                if ui
                    .selectable_label(choice == Some(i), text)
                    .on_hover_text(crate::i18n::text("repo-workflows-surgery-picker-1c448d60"))
                    .clicked()
                {
                    *chosen = Some((h, Some(i)));
                }
            }
        });
}
