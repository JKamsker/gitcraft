//! Workspace repository row interaction.

use super::*;

pub(super) enum RowClick {
    Open,
    Remove,
}

/// Room kept at the row's right end for the remove button, shown or not.
const REMOVE_ROOM: f32 = 30.0;

/// One repository of a workspace: its name and machine on the left, how it stands on the right.
pub(super) fn workspace_row(
    ui: &mut Ui,
    path: &Path,
    state: Option<&RepoState>,
    t: &Tokens,
) -> Option<RowClick> {
    let (_, rect) = ui.allocate_space(vec2(ui.available_width(), 36.0));
    let resp = ui.interact(
        rect,
        ui.id().with(("workspace-repository", path)),
        Sense::click(),
    );
    crate::a11y::a11y(&resp, egui::WidgetType::Button, || row_tooltip(path, state));
    let hovered = resp.hovered() || ui.rect_contains_pointer(rect);
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius, t.hover);
    }
    let y = rect.center().y;
    let icon = if crate::remote::is_remote(path) {
        "server"
    } else {
        "folder-git-2"
    };
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 14.0, y), vec2(16.0, 16.0)),
        icon,
        14.0,
        t.icon,
    );
    // The status gets at most 60 % of the row; the name keeps the rest.
    let text_left = rect.left() + 30.0;
    let status_rect = Rect::from_min_max(
        pos2(text_left + (rect.right() - text_left) * 0.4, rect.top()),
        pos2(rect.right() - REMOVE_ROOM, rect.bottom()),
    );
    let connected = crate::remote::parse(path).is_none_or(|t| crate::remote::is_connected(&t.host));
    let status = status_parts(state, connected, t);
    let status_width = paint_parts(ui, status_rect, status);
    let left = ui.painter_at(rect.with_max_x(status_rect.right() - status_width - 12.0));
    left.text(
        pos2(text_left, y - 7.0),
        Align2::LEFT_CENTER,
        repo_name(path),
        theme::medium(12.5),
        t.text,
    );
    left.text(
        pos2(text_left, y + 8.0),
        Align2::LEFT_CENTER,
        crate::remote::display(path),
        egui::FontId::proportional(10.5),
        t.text_faint,
    );
    let mut click = None;
    if hovered
        || crate::a11y::keyboard_on(&resp)
        || ui.memory(|m| m.has_focus(resp.id.with("remove")))
    {
        let x_rect = Rect::from_center_size(pos2(rect.right() - 12.0, y), vec2(20.0, 20.0));
        let x = ui
            .interact(x_rect, resp.id.with("remove"), Sense::click())
            .on_hover_text(crate::i18n::text("interaction-workspace-remove"));
        crate::a11y::a11y(&x, egui::WidgetType::Button, || {
            crate::i18n::message(
                "interaction-workspace-remove-name",
                &[("repository", repo_name(path).into())],
            )
        });
        if x.hovered() {
            ui.painter().rect_filled(x_rect, t.radius_sm, t.pressed);
        }
        icons::paint(ui, x_rect, "x", 12.0, t.text_faint);
        if x.clicked() {
            click = Some(RowClick::Remove);
        }
    }
    if click.is_none() && resp.on_hover_text(row_tooltip(path, state)).clicked() {
        click = Some(RowClick::Open);
    }
    click
}

/// A piece of a row's status: an icon (or none) and a text, in a colour.
pub(super) struct Part {
    icon: Option<&'static str>,
    text: String,
    color: Color32,
    /// Can be cut in the middle when the row is short of room (a branch's name).
    shrinks: bool,
}

pub(super) fn part(icon: Option<&'static str>, text: impl Into<String>, color: Color32) -> Part {
    Part {
        icon,
        text: text.into(),
        color,
        shrinks: false,
    }
}

/// What a row says of its repository, left to right.
pub(super) fn status_parts(state: Option<&RepoState>, connected: bool, t: &Tokens) -> Vec<Part> {
    let Some(state) = state.filter(|s| s.seen.is_some() || s.busy.is_some()) else {
        return vec![if connected {
            part(None, "…", t.text_faint)
        } else {
            part(
                None,
                crate::i18n::text("interaction-workspace-disconnected"),
                t.text_faint,
            )
        }];
    };
    let mut parts = Vec::new();
    match state.busy {
        Some(Busy::Fetching) => parts.push(part(
            Some("cloud-download"),
            crate::i18n::text("interaction-workspace-fetching"),
            t.text_dim,
        )),
        Some(Busy::Looking) if state.seen.is_none() => {
            parts.push(part(
                None,
                crate::i18n::text("interaction-workspace-looking"),
                t.text_faint,
            ));
        }
        _ => {}
    }
    if let Some((e, _)) = &state.fetch_error {
        let words = crate::i18n::message(
            "interaction-workspace-fetch-error",
            &[("reason", across::display_reason(e).into())],
        );
        parts.push(part(Some("circle-alert"), words, t.danger));
    }
    match &state.seen {
        Some(Ok(o)) => parts.extend(overview_parts(o, git::now(), t)),
        Some(Err(e)) => parts.push(part(
            Some("circle-alert"),
            across::display_reason(e),
            t.danger,
        )),
        None => {}
    }
    parts
}

/// The branch, the uncommitted files, ahead / behind and the last fetch of `o`.
pub(super) fn overview_parts(o: &Overview, now: i64, t: &Tokens) -> Vec<Part> {
    let branch = match &o.branch {
        Some(b) => b.clone(),
        None if o.head.is_empty() => {
            crate::i18n::text("interaction-workspace-no-commits").to_owned()
        }
        None => crate::i18n::message(
            "interaction-workspace-detached",
            &[("commit", o.head.as_str().into())],
        ),
    };
    let mut parts = vec![Part {
        shrinks: true,
        ..part(Some("git-branch"), branch, t.text_dim)
    }];
    if o.changed > 0 {
        parts.push(part(None, changed_text(o.changed), t.warning));
    }
    if o.ahead > 0 {
        parts.push(part(
            Some("arrow-up"),
            crate::i18n::number(o.ahead),
            t.text_dim,
        ));
    }
    if o.behind > 0 {
        parts.push(part(
            Some("arrow-down"),
            crate::i18n::number(o.behind),
            t.text_dim,
        ));
    }
    if o.has_remote {
        parts.push(part(None, fetched_text(o.last_fetch, now), t.text_faint));
    } else {
        parts.push(part(
            None,
            crate::i18n::text("interaction-workspace-no-remote"),
            t.text_faint,
        ));
    }
    parts
}

/// "1 changed", "12 changed".
pub(in crate::app) fn changed_text(n: usize) -> String {
    crate::i18n::message("interaction-workspace-changed", &[("count", n.into())])
}

/// A localized relative fetch age, or the no-fetch fallback.
pub(in crate::app) fn fetched_text(at: Option<i64>, now: i64) -> String {
    match at {
        Some(at) => crate::i18n::message(
            "interaction-workspace-fetched",
            &[("time", git::relative_time(at, now).into())],
        ),
        None => crate::i18n::text("interaction-workspace-never").to_owned(),
    }
}

/// `text` cut to `keep` characters with "…" in the middle (both ends say the most).
pub(super) fn cut_middle(text: &str, keep: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= keep {
        return text.to_owned();
    }
    let keep = keep.max(2) - 1;
    let head = keep.div_ceil(2);
    let tail = keep - head;
    let mut s: String = chars[..head].iter().collect();
    s.push('…');
    s.extend(&chars[chars.len() - tail..]);
    s
}

const PART_GAP: f32 = 10.0;
const PART_ICON: f32 = 12.0;

/// How wide `parts` paint.
pub(super) fn parts_width(ui: &Ui, parts: &[Part], font: &egui::FontId) -> f32 {
    parts
        .iter()
        .map(|p| {
            let text = ui
                .painter()
                .layout_no_wrap(p.text.clone(), font.clone(), p.color)
                .size()
                .x;
            text + if p.icon.is_some() {
                PART_ICON + 3.0
            } else {
                0.0
            } + PART_GAP
        })
        .sum()
}

/// Paints `parts` right-aligned in `rect`, cutting the branch's name when they don't fit;
/// returns the width they took.
pub(super) fn paint_parts(ui: &Ui, rect: Rect, mut parts: Vec<Part>) -> f32 {
    let font = egui::FontId::proportional(11.5);
    let full: Vec<String> = parts.iter().map(|p| p.text.clone()).collect();
    let mut keep = usize::MAX;
    while parts_width(ui, &parts, &font) > rect.width() {
        let Some((i, p)) = parts
            .iter_mut()
            .enumerate()
            .find(|(_, p)| p.shrinks && p.text.chars().count() > 6)
        else {
            break;
        };
        keep = keep.min(p.text.chars().count()).saturating_sub(4);
        p.text = cut_middle(&full[i], keep);
    }
    let painter = ui.painter_at(rect);
    let mut x = rect.right();
    for p in parts.iter().rev() {
        let galley = painter.layout_no_wrap(p.text.clone(), font.clone(), p.color);
        x -= galley.size().x;
        painter.galley(
            pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            p.color,
        );
        if let Some(icon) = p.icon {
            x -= PART_ICON + 3.0;
            let at = Rect::from_min_size(
                pos2(x, rect.center().y - PART_ICON / 2.0),
                vec2(PART_ICON, PART_ICON),
            );
            if rect.contains_rect(at) {
                icons::paint(ui, at, icon, PART_ICON, p.color);
            }
        }
        x -= PART_GAP;
    }
    (rect.right() - x).min(rect.width())
}

/// The row's whole story on hover: the path, and why a look or fetch failed, in full.
pub(super) fn row_tooltip(path: &Path, state: Option<&RepoState>) -> String {
    let mut tip = crate::remote::display(path);
    let mut failed = false;
    if let Some(state) = state {
        if let Some(Err(e)) = &state.seen {
            tip.push_str("\n\n");
            tip.push_str(&crate::i18n::message(
                "interaction-workspace-look-error",
                &[("reason", across::display_error(e).into())],
            ));
            failed = true;
        }
        if let Some((e, _)) = &state.fetch_error {
            tip.push_str("\n\n");
            tip.push_str(&crate::i18n::message(
                "interaction-workspace-fetch-failed",
                &[("reason", across::display_error(e).into())],
            ));
        }
        if let Some(Ok(o)) = &state.seen
            && !o.upstream.is_empty()
        {
            tip.push('\n');
            tip.push_str(&crate::i18n::message(
                "interaction-workspace-tracks",
                &[("upstream", o.upstream.as_str().into())],
            ));
        }
    }
    if !failed {
        tip.push('\n');
        tip.push_str(crate::i18n::text("interaction-workspace-click-open"));
    }
    tip
}

/// The folder's name, or the whole path for a root.
pub(in crate::app) fn repo_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| crate::remote::display(path))
}

#[cfg(test)]
impl Part {
    pub(super) fn into_text(self) -> String {
        self.text
    }
}
