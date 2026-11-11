//! Git vocabulary on hover, including text drawn by custom painters. Inspect only the
//! pointer's topmost text layer after a pause; no filesystem, git or network work here.

use egui::{Context, Id, LayerId, Order, Pos2, Shape};

const TERMS: &[(&str, &str)] = &[
    (
        "HEAD",
        "The commit you have checked out. Usually HEAD follows a branch; a detached HEAD points directly to a commit, so new commits need a branch to keep them reachable.",
    ),
    (
        "upstream",
        "The remote branch a local branch tracks. Pull reads from it, and ahead / behind counts compare with it. A separate push remote can send your work elsewhere.",
    ),
    (
        "fast-forward",
        "Move a branch to a later commit without making a merge commit. This is possible only when the branch has no commits of its own beyond the other history.",
    ),
    (
        "rebase",
        "Replay a branch's commits on a new base. Their hashes change. Avoid rewriting history others use unless you have agreed to it.",
    ),
    (
        "merge",
        "Combine histories. When both branches moved on, a merge commit records both parents. Conflicts need a decision before the merge can finish.",
    ),
    (
        "index",
        "Git's staging area: the exact file versions that will go into your next commit. Later edits in the working tree stay separate until staged.",
    ),
    (
        "staged",
        "Changes saved in the index for the next commit. Unstaged edits are still only in the working tree.",
    ),
    (
        "unstaged",
        "Edits in the working tree that have not been put in the index for the next commit.",
    ),
    (
        "stash",
        "A saved set of uncommitted changes you can apply later. It is local to this repository and is not sent by an ordinary push.",
    ),
    (
        "cherry-pick",
        "Apply one existing commit's changes as a new commit on the current branch. The new commit has its own hash.",
    ),
    (
        "revert",
        "Make a new commit that reverses an earlier commit's changes. The original stays in history, so this is suitable for undoing published work.",
    ),
];

/// A whole-word match at a character position, not a byte index. Hyphenated terms stay whole.
fn term_at(text: &str, at: usize) -> Option<(&'static str, &'static str)> {
    let chars: Vec<char> = text.chars().collect();
    let word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-');
    let mut index = at.min(chars.len().saturating_sub(1));
    if !chars.get(index).is_some_and(|&c| word(c)) {
        index = index.checked_sub(1)?;
    }
    if !chars.get(index).is_some_and(|&c| word(c)) {
        return None;
    }
    let mut start = index;
    let mut end = index + 1;
    while start > 0 && word(chars[start - 1]) {
        start -= 1;
    }
    while end < chars.len() && word(chars[end]) {
        end += 1;
    }
    let text: String = chars[start..end].iter().collect();
    TERMS
        .iter()
        .copied()
        .find(|(term, _)| text.eq_ignore_ascii_case(term))
}

fn on_shape(shape: &Shape, point: Pos2) -> Option<(&'static str, &'static str)> {
    match shape {
        Shape::Text(text) if text.angle == 0.0 && text.visual_bounding_rect().contains(point) => {
            let cursor = text.galley.cursor_from_pos(point - text.pos);
            term_at(text.galley.text(), cursor.index.0)
        }
        Shape::Vec(shapes) => shapes.iter().rev().find_map(|shape| on_shape(shape, point)),
        _ => None,
    }
}

pub(crate) fn hover(ctx: &Context) {
    let Some(point) = ctx.pointer_hover_pos() else {
        return;
    };
    // Do not interrupt text selection, active typing, scrolling, or a drag. A focused
    // but idle text field does not turn vocabulary off for the rest of the window.
    let typed_at = Id::new("glossary-last-key");
    let (now, typing) = ctx.input(|i| {
        (
            i.time,
            i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Text(_)
                        | egui::Event::Paste(_)
                        | egui::Event::Key { pressed: true, .. }
                )
            }),
        )
    });
    if typing {
        ctx.data_mut(|d| d.insert_temp(typed_at, now));
    }
    let recently_typed = ctx
        .data(|d| d.get_temp::<f64>(typed_at))
        .is_some_and(|at| now - at < 0.7);
    if recently_typed || ctx.input(|i| i.pointer.any_down() || i.time_since_last_scroll() < 0.7) {
        return;
    }
    let waited = ctx.input(|i| i.pointer.time_since_last_movement());
    if waited < 0.7 {
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(0.7 - waited));
        return;
    }
    let Some(layer) = ctx.layer_id_at(point) else {
        return;
    };
    if matches!(layer.order, Order::Tooltip | Order::Debug) {
        return;
    }
    let local_point = ctx
        .layer_transform_from_global(layer)
        .map_or(point, |t| t * point);
    let found = ctx.graphics(|graphics| {
        let entries = graphics.get(layer)?;
        entries
            .all_entries()
            .filter(|entry| entry.clip_rect.contains(local_point))
            .filter_map(|entry| on_shape(&entry.shape, local_point))
            .last()
    });
    if let Some((term, explanation)) = found {
        let id = Id::new("git-glossary");
        // Give the glossary its own parent layer, so it cannot replace a widget's existing
        // tooltip. Anchor outside the other tooltips that were drawn this pass.
        let own_tip = egui::Tooltip::tooltip_id(id, 0);
        let layers: Vec<_> = ctx.memory(|m| {
            m.layer_ids()
                .filter(|l| l.order == Order::Tooltip && l.id != own_tip)
                .collect()
        });
        let other_tips = layers
            .into_iter()
            .filter(|&l| ctx.graphics(|g| g.get(l).is_some_and(|p| !p.is_empty())))
            .filter_map(|l| egui::AreaState::load(ctx, l.id).map(|s| s.rect()))
            .reduce(|a, b| a.union(b));
        let anchor = other_tips.map_or(egui::PopupAnchor::Pointer, egui::PopupAnchor::ParentRect);
        egui::Tooltip::always_open(
            ctx.clone(),
            LayerId::new(Order::Tooltip, Id::new("glossary-parent")),
            id,
            anchor,
        )
        .show(|ui| {
            ui.strong(format!("Git: {term}"));
            ui.label(explanation);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_uses_words_and_unicode_character_positions() {
        for (text, position, expected) in [
            ("HEAD", 0, Some("HEAD")),
            ("Detached HEAD at abc", 10, Some("HEAD")),
            ("é rebase this", 4, Some("rebase")),
            ("only fast-forward", 10, Some("fast-forward")),
            ("upstream/main", 4, Some("upstream")),
            ("rebase-merge", 3, None),
            ("ahead", 2, None),
            ("header", 2, None),
            ("", 0, None),
        ] {
            assert_eq!(term_at(text, position).map(|p| p.0), expected, "{text}");
        }
    }

    #[test]
    fn painted_text_only_matches_under_the_pointer() {
        let ctx = Context::default();
        let mut shape = None;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let galley = ui.ctx().fonts_mut(|f| {
                f.layout_no_wrap(
                    "HEAD".to_owned(),
                    egui::FontId::proportional(16.0),
                    egui::Color32::WHITE,
                )
            });
            shape = Some(Shape::galley(
                egui::pos2(20.0, 30.0),
                galley,
                egui::Color32::WHITE,
            ));
        });
        output.textures_delta.clear();
        let Some(shape) = shape else {
            return;
        };
        assert_eq!(
            on_shape(&shape, egui::pos2(23.0, 37.0)).map(|p| p.0),
            Some("HEAD")
        );
        assert_eq!(on_shape(&shape, egui::pos2(5.0, 5.0)), None);
    }
}

#[cfg(test)]
mod hover_tests {
    use super::*;
    use egui_kittest::kittest::Queryable as _;

    #[test]
    fn a_real_ui_hover_shows_the_glossary_after_a_pause() {
        let mut harness = egui_kittest::Harness::builder()
            .with_size([640.0, 480.0])
            .build_ui(|ui| {
                ui.label("HEAD").on_hover_text("The existing explanation");
                hover(ui.ctx());
            });
        harness.get_by_label("HEAD").hover();
        for _ in 0..60 {
            harness.step();
        }
        let _ = harness.get_by_label("Git: HEAD");
        let _ = harness.get_by_label("The existing explanation");
    }
}
