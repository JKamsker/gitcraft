use super::*;

/// The revisions of a commit's diff (or a comparison of two) its lines can link to: the file
/// after the change, and before it. `None` for changes not committed, which no forge has.
pub(super) fn sides(key: &DiffKey) -> Option<(String, String)> {
    match key {
        DiffKey::Commit { hash, .. } => Some((hash.clone(), format!("{hash}^"))),
        DiffKey::Range {
            from, to: Some(to), ..
        } => Some((to.clone(), from.clone())),
        DiffKey::Remerge { .. } | DiffKey::Range { to: None, .. } | DiffKey::Work(_) => None,
    }
}

/// "Lines 4–9" (and the others, when the lines moved apart): those the diff is limited to.
pub(super) fn lines_chip(ranges: &[crate::git::LineRange]) -> String {
    let parts: Vec<String> = ranges
        .iter()
        .map(|&(a, b)| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}–{b}")
            }
        })
        .collect();
    crate::i18n::message(
        if ranges.len() == 1 && ranges[0].0 == ranges[0].1 {
            "repo-core-line-range-one"
        } else {
            "repo-core-line-range-many"
        },
        &[("range", parts.join(", ").into())],
    )
}

pub(super) fn wrap_control(ui: &mut Ui, acts: &mut Vec<Action>) {
    let wrap = crate::settings::Settings::get(ui.ctx()).diff_wrap;
    if ui
        .selectable_label(wrap, crate::i18n::text("repo-core-1785"))
        .on_hover_text(crate::i18n::text("repo-core-2387"))
        .clicked()
    {
        acts.push(Action::DiffWrap(!wrap));
    }
}

/// Whitespace, whole file, side by side and unified toggles, laid out right to left.
pub(super) fn view_options(
    ui: &mut Ui,
    t: &Tokens,
    (split, ignore_ws, whole_file): (bool, bool, bool),
    acts: &mut Vec<Action>,
) {
    let ws = icons::small_button(
        ui,
        "pilcrow",
        if ignore_ws {
            crate::i18n::text("repo-core-2389")
        } else {
            crate::i18n::text("repo-core-2390")
        },
    );
    if ignore_ws {
        widgets::active_underline(ui, ws.rect, t.accent);
    }
    if ws.clicked() {
        acts.push(Action::SetDiffOptions {
            split,
            ignore_ws: !ignore_ws,
            whole_file,
        });
    }
    let whole = icons::small_button(
        ui,
        "unfold-vertical",
        if whole_file {
            crate::i18n::text("repo-core-2392")
        } else {
            crate::i18n::text("repo-core-2393")
        },
    );
    if whole_file {
        widgets::active_underline(ui, whole.rect, t.accent);
    }
    if whole.clicked() {
        acts.push(Action::SetDiffOptions {
            split,
            ignore_ws,
            whole_file: !whole_file,
        });
    }
    let s = icons::small_button(ui, "columns-2", crate::i18n::text("repo-core-2395"));
    if split {
        widgets::active_underline(ui, s.rect, t.accent);
    }
    if s.clicked() {
        acts.push(Action::SetDiffOptions {
            split: true,
            ignore_ws,
            whole_file,
        });
    }
    let u = icons::small_button(ui, "rows-2", crate::i18n::text("repo-core-2397"));
    if !split {
        widgets::active_underline(ui, u.rect, t.accent);
    }
    if u.clicked() {
        acts.push(Action::SetDiffOptions {
            split: false,
            ignore_ws,
            whole_file,
        });
    }
}

/// The previous and next change buttons, laid out right to left.
pub(super) fn change_steps(ui: &mut Ui, acts: &mut Vec<Action>) {
    if icons::small_button(
        ui,
        "arrow-down",
        &crate::i18n::message(
            "repo-core-2399",
            &[
                (
                    "NextChange",
                    crate::keymap::label(ui.ctx(), crate::keymap::Command::NextChange)
                        .to_string()
                        .into(),
                ),
                (
                    "NextChangeAlt",
                    crate::keymap::label(ui.ctx(), crate::keymap::Command::NextChangeAlt)
                        .to_string()
                        .into(),
                ),
            ],
        ),
    )
    .clicked()
    {
        acts.push(Action::DiffStep(1));
    }
    if icons::small_button(
        ui,
        "arrow-up",
        &crate::i18n::message(
            "repo-core-2401",
            &[
                (
                    "PreviousChange",
                    crate::keymap::label(ui.ctx(), crate::keymap::Command::PreviousChange)
                        .to_string()
                        .into(),
                ),
                (
                    "PreviousChangeAlt",
                    crate::keymap::label(ui.ctx(), crate::keymap::Command::PreviousChangeAlt)
                        .to_string()
                        .into(),
                ),
            ],
        ),
    )
    .clicked()
    {
        acts.push(Action::DiffStep(-1));
    }
}
