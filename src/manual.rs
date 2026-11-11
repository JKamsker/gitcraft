//! The manual (F1, or the book in the title bar): what gitcraft does and its keyboard shortcuts.

use egui::{RichText, Ui};

mod content;

use content::{TIPS, features};

use crate::keymap;
use crate::theme::{self, Tokens};

/// Ctrl, or Cmd on macOS, as the shortcuts are written.
pub(crate) fn command() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

/// The "Did you know" tips, as written (with `Ctrl`).
pub(crate) fn tips() -> &'static [&'static str] {
    use std::sync::OnceLock;
    static ENGLISH: OnceLock<Vec<&'static str>> = OnceLock::new();
    static GERMAN: OnceLock<Vec<&'static str>> = OnceLock::new();
    let cache = match crate::i18n::language() {
        crate::i18n::Language::English => &ENGLISH,
        crate::i18n::Language::German => &GERMAN,
    };
    cache.get_or_init(|| TIPS.iter().map(|&id| crate::i18n::text(id)).collect())
}

/// Keyboard and mouse shortcuts of the views, besides the main actions' keys in
/// [`crate::keymap`]: (keys, what they do). `Cmd` stands for Ctrl outside macOS.
pub(crate) const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "interaction-manual-group-keyboard-navigation",
        &[
            (
                "interaction-manual-shortcut-mouse-back-forward",
                "interaction-manual-shortcut-back-forward-through-recent-views",
            ),
            (
                "Tab / Shift+Tab",
                "interaction-manual-shortcut-next-previous-button-row-or-field",
            ),
            (
                "Enter / Space",
                "interaction-manual-shortcut-click-the-focused-button-or-row",
            ),
        ],
    ),
    (
        "interaction-manual-group-start-page",
        &[
            (
                "interaction-manual-shortcut-type",
                "interaction-manual-shortcut-filter-the-recent-repositories",
            ),
            (
                "↑ / ↓",
                "interaction-manual-shortcut-pick-a-recent-repository",
            ),
            ("Enter", "interaction-manual-shortcut-open-the-one-picked"),
            ("Esc", "interaction-manual-shortcut-clear-the-filter"),
        ],
    ),
    (
        "interaction-manual-group-graph",
        &[
            (
                "Enter / Shift+Enter",
                "interaction-manual-shortcut-next-previous-search-result",
            ),
            (
                "interaction-manual-shortcut-then-tab-or-enter",
                "interaction-manual-shortcut-pick-a-search-suggestion",
            ),
            (
                "interaction-manual-shortcut-cmd-click",
                "interaction-manual-shortcut-compare-with-the-selected-commit",
            ),
            (
                "interaction-manual-shortcut-double-click-a-pill",
                "interaction-manual-shortcut-check-out-that-branch-or-tag",
            ),
            (
                "interaction-manual-shortcut-alt-over-rebase-reset-hard-pull",
                "interaction-manual-shortcut-preview-the-result-in-the-graph-ghost-run",
            ),
            (
                "interaction-manual-shortcut-drag-a-pill-or-a-commit",
                "interaction-manual-shortcut-merge-rebase-fast-forward-cherry-pick-move-or-squash",
            ),
            (
                "Esc",
                "interaction-manual-shortcut-close-the-find-bar-diff-comparison-or-search",
            ),
        ],
    ),
    (
        "interaction-manual-group-changes",
        &[
            (
                "interaction-manual-shortcut-click-shift-click",
                "interaction-manual-shortcut-pick-lines-in-a-diff-or-a-file-or-a-range-of-them",
            ),
            (
                "interaction-manual-shortcut-click-shift-click",
                "interaction-manual-shortcut-pick-lines-of-a-file-to-link-to-view-file-or-file-in-a-commit-s-diff",
            ),
            (
                "interaction-manual-shortcut-cmd-click-a-file",
                "interaction-manual-shortcut-pick-it-to-stash-or-unpick-it",
            ),
        ],
    ),
    (
        "interaction-manual-group-find-in-a-diff-file-or-blame",
        &[
            (
                "Enter / Shift+Enter",
                "interaction-manual-shortcut-next-previous-match-while-the-find-bar-is-open",
            ),
            ("Esc", "interaction-manual-shortcut-close-the-find-bar"),
        ],
    ),
    (
        "interaction-manual-group-terminal",
        &[
            (
                "Cmd+C",
                "interaction-manual-shortcut-copy-the-selection-without-one-it-goes-to-the-shell",
            ),
            (
                "Cmd+Shift+C",
                "interaction-manual-shortcut-copy-the-selection",
            ),
        ],
    ),
];

/// Every group of shortcuts, as the manual and Settings → Keyboard shortcuts list them: the
/// window's and a repository tab's keys from [`crate::keymap`], then the views' own. Each row:
/// the keys as [`key_combo`] draws them (Ctrl or Cmd already written), what they do.
pub(crate) fn shortcut_groups(
    ctx: &egui::Context,
) -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    let from_keymap = |rows: &[keymap::Row]| {
        rows.iter()
            .map(|r| (keymap::row_keys(ctx, r), crate::i18n::text(r.what)))
            .collect::<Vec<_>>()
    };
    let mut groups = vec![
        (
            crate::i18n::text("interaction-manual-group-window"),
            from_keymap(keymap::WINDOW_ROWS),
        ),
        (
            crate::i18n::text("interaction-manual-group-repository"),
            from_keymap(keymap::REPOSITORY_ROWS),
        ),
        (
            crate::i18n::text("interaction-manual-group-contextual-actions"),
            from_keymap(keymap::LOCAL_ROWS),
        ),
    ];
    groups.extend(SHORTCUTS.iter().map(|(title, keys)| {
        let keys = keys
            .iter()
            .map(|(combo, what)| {
                (
                    keymap::written(if combo.starts_with("interaction-manual-") {
                        crate::i18n::text(combo)
                    } else {
                        combo
                    }),
                    crate::i18n::text(what),
                )
            })
            .collect();
        (crate::i18n::text(title), keys)
    }));
    groups
}

/// Draws the manual's contents.
pub fn ui(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let heading = |ui: &mut Ui, text: &str| {
        ui.add_space(10.0);
        ui.label(
            RichText::new(text.to_uppercase())
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        ui.add_space(2.0);
    };

    for (title, points) in features() {
        heading(ui, crate::i18n::text(title));
        for point in points {
            // The bullet on the left, the text wrapping beside it.
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new("•").color(t.text_faint));
                ui.add(
                    egui::Label::new(
                        RichText::new(crate::i18n::text(point).replace("Ctrl", command()))
                            .color(t.text_dim),
                    )
                    .wrap(),
                );
            });
        }
    }

    ui.add_space(8.0);
    ui.separator();
    shortcut_ui(ui);
}

/// The same translated tables shown by the manual, isolated from its long feature prose.
fn shortcut_ui(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let heading = |ui: &mut Ui, text: &str| {
        ui.add_space(10.0);
        ui.label(
            RichText::new(text.to_uppercase())
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        ui.add_space(2.0);
    };
    for (group_index, (title, keys)) in shortcut_groups(ui.ctx()).into_iter().enumerate() {
        heading(
            ui,
            &crate::i18n::message(
                "interaction-manual-shortcut-heading",
                &[("group", title.into())],
            ),
        );
        egui::Grid::new(("manual_keys", group_index))
            .num_columns(2)
            .spacing([16.0, 6.0])
            .min_col_width(170.0)
            .show(ui, |ui| {
                for (combo, what) in &keys {
                    ui.horizontal(|ui| key_combo(ui, combo));
                    ui.label(RichText::new(*what).color(t.text_dim));
                    ui.end_row();
                }
            });
    }
}

/// Key combinations, `a / b`, each drawn like a keycap.
pub(crate) fn key_combo(ui: &mut Ui, combo: &str) {
    let t = Tokens::get(ui.ctx());
    ui.spacing_mut().item_spacing.x = 4.0;
    for (n, part) in combo.split(" / ").enumerate() {
        if n > 0 {
            ui.label(RichText::new("/").color(t.text_faint));
        }
        key_chip(ui, part);
    }
}

/// A key combination drawn like a keycap.
fn key_chip(ui: &mut Ui, keys: &str) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.field)
        .stroke(egui::Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(keys).font(theme::mono(11.5)).color(t.text));
        });
}

#[cfg(test)]
mod feature_localization_tests;

#[cfg(test)]
mod localization_tests;
