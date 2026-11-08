//! The manual (F1, or the book in the title bar): what gitcraft does and its keyboard shortcuts.

use egui::{RichText, Ui};

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

/// Feature headings and whole paragraphs, stored as stable Fluent IDs.
/// Paragraph suffixes are permanent identities; do not renumber when inserting new prose.
const FEATURES: &[(&str, &[&str])] = &[
    (
        "interaction-manual-feature-repositories-tabs-title",
        &[
            "interaction-manual-feature-repositories-tabs-01",
            "interaction-manual-feature-repositories-tabs-02",
            "interaction-manual-feature-repositories-tabs-03",
            "interaction-manual-feature-repositories-tabs-04",
            "interaction-manual-feature-repositories-tabs-05",
            "interaction-manual-feature-repositories-tabs-06",
            "interaction-manual-feature-repositories-tabs-07",
            "interaction-manual-feature-repositories-tabs-08",
            "interaction-manual-feature-repositories-tabs-09",
            "interaction-manual-feature-repositories-tabs-10",
            "interaction-manual-feature-repositories-tabs-11",
            "interaction-manual-feature-repositories-tabs-12",
            "interaction-manual-feature-repositories-tabs-13",
            "interaction-manual-feature-repositories-tabs-14",
            "interaction-manual-feature-repositories-tabs-15",
            "interaction-manual-feature-repositories-tabs-16",
            "interaction-manual-feature-repositories-tabs-17",
            "interaction-manual-feature-repositories-tabs-18",
            "interaction-manual-feature-repositories-tabs-19",
            "interaction-manual-feature-repositories-tabs-20",
            "interaction-manual-feature-repositories-tabs-21",
            "interaction-manual-feature-repositories-tabs-22",
            "interaction-manual-feature-repositories-tabs-23",
        ],
    ),
    (
        "interaction-manual-feature-portable-title",
        &[
            "interaction-manual-feature-portable-01",
            "interaction-manual-feature-portable-02",
        ],
    ),
    (
        "interaction-manual-feature-palette-title",
        &[
            "interaction-manual-feature-palette-01",
            "interaction-manual-feature-palette-02",
            "interaction-manual-feature-palette-03",
            "interaction-manual-feature-palette-04",
        ],
    ),
    (
        "interaction-manual-feature-terminal-file-manager-title",
        &[
            "interaction-manual-feature-terminal-file-manager-01",
            "interaction-manual-feature-terminal-file-manager-02",
            "interaction-manual-feature-terminal-file-manager-03",
            "interaction-manual-feature-terminal-file-manager-04",
        ],
    ),
    (
        "interaction-manual-feature-gerrit-title",
        &[
            "interaction-manual-feature-gerrit-01",
            "interaction-manual-feature-gerrit-02",
            "interaction-manual-feature-gerrit-03",
            "interaction-manual-feature-gerrit-04",
        ],
    ),
    (
        "interaction-manual-feature-graph-title",
        &[
            "interaction-manual-feature-graph-01",
            "interaction-manual-feature-graph-02",
            "interaction-manual-feature-graph-03",
            "interaction-manual-feature-graph-04",
            "interaction-manual-feature-graph-05",
            "interaction-manual-feature-graph-06",
            "interaction-manual-feature-graph-07",
            "interaction-manual-feature-graph-08",
            "interaction-manual-feature-graph-09",
            "interaction-manual-feature-graph-10",
            "interaction-manual-feature-graph-11",
            "interaction-manual-feature-graph-12",
            "interaction-manual-feature-graph-13",
            "interaction-manual-feature-graph-14",
            "interaction-manual-feature-graph-15",
            "interaction-manual-feature-graph-16",
            "interaction-manual-feature-graph-17",
            "interaction-manual-feature-graph-18",
            "interaction-manual-feature-graph-19",
            "interaction-manual-feature-graph-20",
            "interaction-manual-feature-graph-21",
            "interaction-manual-feature-graph-22",
            "interaction-manual-feature-graph-23",
            "interaction-manual-feature-graph-24",
        ],
    ),
    (
        "interaction-manual-feature-compare-title",
        &[
            "interaction-manual-feature-compare-01",
            "interaction-manual-feature-compare-02",
            "interaction-manual-feature-compare-03",
        ],
    ),
    (
        "interaction-manual-feature-updates-title",
        &[
            "interaction-manual-feature-updates-01",
            "interaction-manual-feature-updates-02",
        ],
    ),
    (
        "interaction-manual-feature-bisect-title",
        &[
            "interaction-manual-feature-bisect-01",
            "interaction-manual-feature-bisect-02",
            "interaction-manual-feature-bisect-03",
            "interaction-manual-feature-bisect-04",
            "interaction-manual-feature-bisect-05",
        ],
    ),
    (
        "interaction-manual-feature-parallel-bisect-title",
        &[
            "interaction-manual-feature-parallel-bisect-01",
            "interaction-manual-feature-parallel-bisect-02",
            "interaction-manual-feature-parallel-bisect-03",
            "interaction-manual-feature-parallel-bisect-04",
            "interaction-manual-feature-parallel-bisect-05",
            "interaction-manual-feature-parallel-bisect-06",
            "interaction-manual-feature-parallel-bisect-07",
        ],
    ),
    (
        "interaction-manual-feature-code-owners-title",
        &[
            "interaction-manual-feature-code-owners-01",
            "interaction-manual-feature-code-owners-02",
            "interaction-manual-feature-code-owners-03",
        ],
    ),
    (
        "interaction-manual-feature-history-tests-title",
        &[
            "interaction-manual-feature-history-tests-01",
            "interaction-manual-feature-history-tests-02",
            "interaction-manual-feature-history-tests-03",
            "interaction-manual-feature-history-tests-04",
        ],
    ),
    (
        "interaction-manual-feature-rehearse-title",
        &[
            "interaction-manual-feature-rehearse-01",
            "interaction-manual-feature-rehearse-02",
            "interaction-manual-feature-rehearse-03",
        ],
    ),
    (
        "interaction-manual-feature-interaction-title",
        &[
            "interaction-manual-feature-interaction-01",
            "interaction-manual-feature-interaction-02",
            "interaction-manual-feature-interaction-03",
            "interaction-manual-feature-interaction-04",
        ],
    ),
    (
        "interaction-manual-feature-changes-commits-title",
        &[
            "interaction-manual-feature-changes-commits-01",
            "interaction-manual-feature-changes-commits-02",
            "interaction-manual-feature-changes-commits-03",
            "interaction-manual-feature-changes-commits-04",
            "interaction-manual-feature-changes-commits-05",
            "interaction-manual-feature-changes-commits-06",
            "interaction-manual-feature-changes-commits-07",
            "interaction-manual-feature-changes-commits-08",
            "interaction-manual-feature-changes-commits-09",
            "interaction-manual-feature-changes-commits-10",
            "interaction-manual-feature-changes-commits-11",
            "interaction-manual-feature-changes-commits-12",
            "interaction-manual-feature-changes-commits-13",
            "interaction-manual-feature-changes-commits-14",
            "interaction-manual-feature-changes-commits-15",
            "interaction-manual-feature-changes-commits-16",
            "interaction-manual-feature-changes-commits-17",
            "interaction-manual-feature-changes-commits-18",
            "interaction-manual-feature-changes-commits-19",
            "interaction-manual-feature-changes-commits-20",
            "interaction-manual-feature-changes-commits-21",
            "interaction-manual-feature-changes-commits-22",
            "interaction-manual-feature-changes-commits-23",
            "interaction-manual-feature-changes-commits-24",
            "interaction-manual-feature-changes-commits-25",
            "interaction-manual-feature-changes-commits-26",
            "interaction-manual-feature-changes-commits-27",
            "interaction-manual-feature-changes-commits-28",
            "interaction-manual-feature-changes-commits-29",
            "interaction-manual-feature-changes-commits-30",
            "interaction-manual-feature-changes-commits-31",
            "interaction-manual-feature-changes-commits-32",
        ],
    ),
    (
        "interaction-manual-feature-agents-mcp-title",
        &[
            "interaction-manual-feature-agents-mcp-01",
            "interaction-manual-feature-agents-mcp-02",
            "interaction-manual-feature-agents-mcp-03",
            "interaction-manual-feature-agents-mcp-04",
            "interaction-manual-feature-agents-mcp-05",
        ],
    ),
    (
        "interaction-manual-feature-timeline-title",
        &[
            "interaction-manual-feature-timeline-01",
            "interaction-manual-feature-timeline-02",
            "interaction-manual-feature-timeline-03",
            "interaction-manual-feature-timeline-04",
            "interaction-manual-feature-timeline-05",
            "interaction-manual-feature-timeline-06",
            "interaction-manual-feature-timeline-07",
        ],
    ),
    (
        "interaction-manual-feature-doctor-title",
        &[
            "interaction-manual-feature-doctor-01",
            "interaction-manual-feature-doctor-02",
            "interaction-manual-feature-doctor-03",
            "interaction-manual-feature-doctor-04",
            "interaction-manual-feature-doctor-05",
            "interaction-manual-feature-doctor-06",
        ],
    ),
    (
        "interaction-manual-feature-branches-remotes-title",
        &[
            "interaction-manual-feature-branches-remotes-01",
            "interaction-manual-feature-branches-remotes-02",
            "interaction-manual-feature-branches-remotes-03",
            "interaction-manual-feature-branches-remotes-04",
            "interaction-manual-feature-branches-remotes-05",
            "interaction-manual-feature-branches-remotes-06",
            "interaction-manual-feature-branches-remotes-07",
            "interaction-manual-feature-branches-remotes-08",
            "interaction-manual-feature-branches-remotes-09",
            "interaction-manual-feature-branches-remotes-10",
            "interaction-manual-feature-branches-remotes-11",
            "interaction-manual-feature-branches-remotes-12",
            "interaction-manual-feature-branches-remotes-13",
            "interaction-manual-feature-branches-remotes-14",
            "interaction-manual-feature-branches-remotes-15",
            "interaction-manual-feature-branches-remotes-16",
            "interaction-manual-feature-branches-remotes-17",
            "interaction-manual-feature-branches-remotes-18",
            "interaction-manual-feature-branches-remotes-19",
            "interaction-manual-feature-branches-remotes-20",
            "interaction-manual-feature-branches-remotes-21",
            "interaction-manual-feature-branches-remotes-22",
            "interaction-manual-feature-branches-remotes-23",
            "interaction-manual-feature-branches-remotes-24",
            "interaction-manual-feature-branches-remotes-25",
            "interaction-manual-feature-branches-remotes-26",
            "interaction-manual-feature-branches-remotes-27",
            "interaction-manual-feature-branches-remotes-28",
            "interaction-manual-feature-branches-remotes-29",
            "interaction-manual-feature-branches-remotes-30",
            "interaction-manual-feature-branches-remotes-31",
            "interaction-manual-feature-branches-remotes-32",
            "interaction-manual-feature-branches-remotes-33",
            "interaction-manual-feature-branches-remotes-34",
            "interaction-manual-feature-branches-remotes-35",
            "interaction-manual-feature-branches-remotes-36",
            "interaction-manual-feature-branches-remotes-37",
            "interaction-manual-feature-branches-remotes-38",
            "interaction-manual-feature-branches-remotes-39",
            "interaction-manual-feature-branches-remotes-40",
            "interaction-manual-feature-branches-remotes-41",
            "interaction-manual-feature-branches-remotes-42",
            "interaction-manual-feature-branches-remotes-43",
            "interaction-manual-feature-branches-remotes-44",
            "interaction-manual-feature-branches-remotes-45",
            "interaction-manual-feature-branches-remotes-46",
            "interaction-manual-feature-branches-remotes-47",
            "interaction-manual-feature-branches-remotes-48",
            "interaction-manual-feature-branches-remotes-49",
        ],
    ),
    (
        "interaction-manual-feature-accessibility-title",
        &[
            "interaction-manual-feature-accessibility-01",
            "interaction-manual-feature-accessibility-02",
            "interaction-manual-feature-accessibility-03",
            "interaction-manual-feature-accessibility-04",
            "interaction-manual-feature-accessibility-05",
        ],
    ),
    (
        "interaction-manual-feature-custom-commands-title",
        &[
            "interaction-manual-feature-custom-commands-01",
            "interaction-manual-feature-custom-commands-02",
            "interaction-manual-feature-custom-commands-03",
            "interaction-manual-feature-custom-commands-04",
            "interaction-manual-feature-custom-commands-05",
        ],
    ),
    (
        "interaction-manual-feature-git-failures-title",
        &[
            "interaction-manual-feature-git-failures-01",
            "interaction-manual-feature-git-failures-02",
            "interaction-manual-feature-git-failures-03",
            "interaction-manual-feature-git-failures-04",
        ],
    ),
    (
        "interaction-manual-feature-maintainers-title",
        &[
            "interaction-manual-feature-maintainers-01",
            "interaction-manual-feature-maintainers-02",
            "interaction-manual-feature-maintainers-03",
            "interaction-manual-feature-maintainers-04",
            "interaction-manual-feature-maintainers-05",
            "interaction-manual-feature-maintainers-06",
            "interaction-manual-feature-maintainers-07",
        ],
    ),
    (
        "interaction-manual-feature-agent-tasks-title",
        &[
            "interaction-manual-feature-agent-tasks-01",
            "interaction-manual-feature-agent-tasks-02",
            "interaction-manual-feature-agent-tasks-03",
            "interaction-manual-feature-agent-tasks-04",
            "interaction-manual-feature-agent-tasks-05",
            "interaction-manual-feature-agent-tasks-06",
        ],
    ),
    (
        "interaction-manual-feature-packages-title",
        &["interaction-manual-feature-packages-01"],
    ),
];

/// The "Did you know" tips under the graph: features above, each said in one short line of its
/// own (`Ctrl` as written in them). Keep them in step with [`FEATURES`].
const TIPS: &[&str] = &[
    "Double-click a branch pill to check that branch out.",
    "Ctrl+click a second commit to list the files that differ between the two.",
    "A branch's menu has “Changes x adds to main”: what it changed, like a pull request.",
    "Hover a branch and click its eye to hide it from the graph; Solo shows only that one.",
    "Right-click a commit or a pill for merge, rebase, cherry-pick, revert, reset and more.",
    "Search can look in the code: switch it to Code to find the commits that added some text.",
    "Right-click the graph's column header to pick its columns or make the graph compact.",
    "In a diff, click the lines you want to stage, unstage or discard only those.",
    "F7 and Alt+↓ go to the next change of a diff; Shift+F7 and Alt+↑ to the previous one.",
    "Double-click a commit's message in the right pane to edit it.",
    "Ctrl+Z undoes commits, merges, rebases, resets, checkouts, branches, tags and stashes.",
    "Ctrl+P opens the command palette: every action, and every branch to check out.",
    "Right-click a file for its history, its blame or the whole file at that commit.",
    "Switch a diff from Diff to File to read the whole file.",
    "Conflicts open in a three-pane editor with Ours, Theirs and Both for each one.",
    "Profiles each have their own tabs, theme, git author and commit signing.",
    "Right-click a tab to give it its own icon and highlight colour.",
    "Ctrl+Shift+T reopens the tab closed last.",
    "Remote on the start page opens a repository in WSL or over SSH.",
    "A pull request's menu has Open in worktree, to work on it in a folder of its own.",
    "Interactive rebase reorders, rewords, squashes or drops commits.",
];

/// The "Did you know" tips, as written (with `Ctrl`).
pub(crate) fn tips() -> &'static [&'static str] {
    TIPS
}

/// Keyboard and mouse shortcuts of the views, besides the main actions' keys in
/// [`crate::keymap`]: (keys, what they do). `Cmd` stands for Ctrl outside macOS.
pub(crate) const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Keyboard navigation",
        &[
            (
                "Mouse back / forward",
                "Back / forward through recent views",
            ),
            ("Tab / Shift+Tab", "Next / previous button, row or field"),
            ("Enter / Space", "Click the focused button or row"),
        ],
    ),
    (
        "Start page",
        &[
            ("Type", "Filter the recent repositories"),
            ("↑ / ↓", "Pick a recent repository"),
            ("Enter", "Open the one picked"),
            ("Esc", "Clear the filter"),
        ],
    ),
    (
        "Graph",
        &[
            ("Enter / Shift+Enter", "Next / previous search result"),
            ("↑ / ↓ then Tab or Enter", "Pick a search suggestion"),
            ("Cmd+click", "Compare with the selected commit"),
            ("Double-click a pill", "Check out that branch or tag"),
            (
                "Alt over Rebase, Reset hard, Pull…",
                "Preview the result in the graph (ghost run)",
            ),
            (
                "Drag a pill or a commit",
                "Merge, rebase, fast-forward, cherry-pick, move or squash",
            ),
            ("Esc", "Close the find bar, diff, comparison or search"),
        ],
    ),
    (
        "Changes",
        &[
            (
                "Click / Shift+click",
                "Pick lines in a diff or a file, or a range of them",
            ),
            (
                "Click / Shift+click",
                "Pick lines of a file to link to (View file, or File in a commit's diff)",
            ),
            ("Cmd+click a file", "Pick it to stash (or unpick it)"),
        ],
    ),
    (
        "Find in a diff, file or blame",
        &[
            (
                "Enter / Shift+Enter",
                "Next / previous match (while the find bar is open)",
            ),
            ("Esc", "Close the find bar"),
        ],
    ),
    (
        "Terminal",
        &[
            (
                "Cmd+C",
                "Copy the selection (without one, it goes to the shell)",
            ),
            ("Cmd+Shift+C", "Copy the selection"),
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
            .map(|r| (keymap::row_keys(ctx, r), r.what))
            .collect::<Vec<_>>()
    };
    let mut groups = vec![
        ("Window", from_keymap(keymap::WINDOW_ROWS)),
        ("Repository", from_keymap(keymap::REPOSITORY_ROWS)),
        ("Contextual actions", from_keymap(keymap::LOCAL_ROWS)),
    ];
    groups.extend(SHORTCUTS.iter().map(|(title, keys)| {
        let keys = keys
            .iter()
            .map(|(combo, what)| (keymap::written(combo), *what))
            .collect();
        (*title, keys)
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

    for (title, points) in FEATURES {
        heading(ui, crate::i18n::text(title));
        for point in *points {
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
    for (title, keys) in shortcut_groups(ui.ctx()) {
        heading(ui, &format!("{title} shortcuts"));
        egui::Grid::new(("manual_keys", title))
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
mod feature_localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use fluent_bundle::{FluentResource, concurrent::FluentBundle};
    use std::collections::BTreeSet;

    const CATALOGS: [(&str, &str); 2] = [
        (
            "en-US",
            include_str!("../locales/en-US/interaction-manual-features.ftl"),
        ),
        (
            "de",
            include_str!("../locales/de/interaction-manual-features.ftl"),
        ),
    ];

    #[test]
    fn every_feature_has_a_paired_whole_message_without_missing_variables() -> Result<(), String> {
        let expected: BTreeSet<_> = FEATURES
            .iter()
            .flat_map(|(title, points)| std::iter::once(*title).chain(points.iter().copied()))
            .collect();
        assert_eq!(FEATURES.len(), 25);
        assert_eq!(FEATURES.iter().map(|(_, p)| p.len()).sum::<usize>(), 219);
        assert_eq!(expected.len(), 244);
        let mut values = Vec::new();
        for (locale, source) in CATALOGS {
            let resource = FluentResource::try_new(source.to_owned())
                .map_err(|(_, errors)| format!("{locale}: {errors:?}"))?;
            let ids: Vec<_> = source
                .lines()
                .filter_map(|line| line.split_once(" = ").map(|(id, _)| id))
                .collect();
            let actual: BTreeSet<_> = ids.iter().copied().collect();
            assert_eq!(ids.len(), actual.len(), "duplicate {locale} feature ID");
            assert_eq!(actual, expected, "{locale} feature coverage");
            let mut bundle = FluentBundle::new_concurrent(vec![
                locale.parse().map_err(|e| format!("{locale}: {e}"))?,
            ]);
            bundle.set_use_isolating(false);
            bundle
                .add_resource(resource)
                .map_err(|e| format!("{locale}: {e:?}"))?;
            let mut translated = Vec::new();
            for id in &expected {
                let message = bundle.get_message(id).ok_or_else(|| id.to_string())?;
                assert!(message.attributes().next().is_none());
                let pattern = message.value().ok_or_else(|| id.to_string())?;
                let mut errors = Vec::new();
                let value = bundle
                    .format_pattern(pattern, None, &mut errors)
                    .into_owned();
                // Every feature is a complete static message. Empty arguments must render
                // without errors in both catalogs, including literal shell $VARIABLES.
                assert!(errors.is_empty(), "{locale}/{id}: {errors:?}");
                assert!(!value.is_empty());
                crate::i18n::set_language(locale);
                assert_eq!(crate::i18n::text(id), value, "embedded {locale}/{id}");
                translated.push(value);
            }
            values.push(translated);
        }
        for ((id, en), de) in expected.iter().zip(&values[0]).zip(&values[1]) {
            if *id != "interaction-manual-feature-gerrit-title"
                && *id != "interaction-manual-feature-graph-title"
                && *id != "interaction-manual-feature-bisect-title"
                && *id != "interaction-manual-feature-updates-title"
            {
                assert_ne!(en, de, "untranslated feature {id}");
            }
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }

    #[test]
    fn actual_manual_feature_labels_render_in_selected_german_and_english() {
        // Only the real manual's immediate-mode UI; no app, repo, reader or native setup.
        let fonts = std::cell::Cell::new(false);
        crate::i18n::set_language("de");
        let mut h = Harness::builder()
            .with_size(egui::vec2(680.0, 100_000.0))
            .build_ui(|ui| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                super::ui(ui);
            });
        h.run();
        h.get_by_label("REPOSITORYS UND TABS");
        h.get_by_label("PAKETE");
        for (_, points) in FEATURES {
            for point in *points {
                h.get_by_label(&crate::i18n::text(point).replace("Ctrl", command()));
            }
        }
        h.get_by_label(
            "Ein Tab pro Repository. Änderungen aus Editor oder Terminal erscheinen von selbst.",
        );
        h.get_by_label("Ein Terminalbereich mit Tabs im Ordner des Repositorys.");
        let german_command =
            crate::i18n::text("interaction-manual-feature-terminal-file-manager-01")
                .replace("Ctrl", command());
        h.get_by_label(&german_command);
        // Accessible label queries observe the selected prose, never the stable IDs.
        assert!(h.query_by_label("One tab per repository. Changes made elsewhere (editor, terminal) show up by themselves.").is_none());
        crate::i18n::set_language("en-US");
        h.run();
        h.get_by_label("REPOSITORIES AND TABS");
        h.get_by_label("PACKAGES");
        for (_, points) in FEATURES {
            for point in *points {
                h.get_by_label(&crate::i18n::text(point).replace("Ctrl", command()));
            }
        }
        h.get_by_label("One tab per repository. Changes made elsewhere (editor, terminal) show up by themselves.");
        h.get_by_label("A terminal panel with tabs, in the repository's folder.");
        let english_command =
            crate::i18n::text("interaction-manual-feature-terminal-file-manager-01")
                .replace("Ctrl", command());
        h.get_by_label(&english_command);
        assert!(
            h.query_by_label("Ein Terminalbereich mit Tabs im Ordner des Repositorys.")
                .is_none()
        );
    }
}
