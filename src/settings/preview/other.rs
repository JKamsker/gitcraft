//! Previews of the other settings: the terminal, pulling, code colours, avatars, diffs and the
//! changed files.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Ui, pos2, vec2};

use super::graph::{initials, name_hash};
use super::{area, draw};
use crate::repo::pull::PullMode;
use crate::settings::Settings;
use crate::theme::{self, Tokens};

/// The terminal with the default shell's prompt, at the font size set, in the theme's
/// colours.
pub(super) fn terminal(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let size = s.terminal_font.clamp(9.0, 20.0);
    let line_h = size * 1.45;
    let shell = s.shell.to_lowercase();
    // The prompt as the shell set writes it by default (the system's: as on Linux or macOS).
    let prompt: Vec<(&str, Color32)> = if shell.contains("pwsh") || shell.contains("powershell") {
        vec![("PS C:\\code\\gitcraft> ", t.text)]
    } else if shell.contains("cmd") {
        vec![("C:\\code\\gitcraft>", t.text)]
    } else if shell.contains("fish") {
        vec![
            ("~/gitcraft", t.ansi[2]),
            (" (main)", t.ansi[5]),
            ("> ", t.text),
        ]
    } else {
        vec![
            ("me@laptop", t.ansi[2]),
            (":", t.text),
            ("~/gitcraft", t.ansi[4]),
            (" (main)", t.ansi[5]),
            ("$ ", t.text),
        ]
    };
    let lines: Vec<Vec<(&str, Color32)>> = vec![
        {
            let mut l = prompt.clone();
            l.push(("git status --short", t.text));
            l
        },
        vec![(" M ", t.ansi[1]), ("src/theme/mod.rs", t.text)],
        vec![("A  ", t.ansi[2]), ("src/theme/catalog.rs", t.text)],
        vec![("?? ", t.ansi[1]), ("themes/my-theme.json", t.text)],
        {
            let mut l = prompt.clone();
            l.push(("git log --oneline -2", t.text));
            l
        },
        vec![
            ("3f9a2c1 ", t.ansi[3]),
            ("(HEAD -> main) ", t.ansi[6]),
            ("Theme previews", t.text),
        ],
        vec![("b81e07d ", t.ansi[3]), ("Merge fix/tab-overflow", t.text)],
        {
            let mut l = prompt;
            l.push(("▌", t.text_dim));
            l
        },
    ];
    let h = 16.0 + lines.len() as f32 * line_h + 34.0;
    let rect = area(ui, width, h);
    let p = ui.painter_at(rect);
    p.rect_filled(rect, t.radius, t.field);
    p.rect_stroke(
        rect,
        t.radius,
        Stroke::new(1.0, t.field_border),
        egui::StrokeKind::Inside,
    );
    let font = theme::mono(size);
    for (i, line) in lines.iter().enumerate() {
        let mut x = rect.left() + 10.0;
        let y = rect.top() + 8.0 + line_h * (i as f32 + 0.5);
        for (text, color) in line {
            let r = p.text(pos2(x, y), Align2::LEFT_CENTER, *text, font.clone(), *color);
            x = r.right();
        }
    }
    // The theme's 16 colours.
    let y = rect.bottom() - 22.0;
    let sw = ((rect.width() - 20.0) / 16.0).min(22.0);
    for (i, c) in t.ansi.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.left() + 10.0 + i as f32 * sw, y),
            vec2(sw - 3.0, 12.0),
        );
        p.rect_filled(r, 2.0, *c);
    }
}

/// A dot of the pull diagrams, its line to the one below, and its label.
struct Dot {
    lane: usize,
    label: &'static str,
    /// Drawn hollow: a commit made anew (a rebase's copy).
    new: bool,
    merge: bool,
}

/// Each way of pulling, side by side: your branch and its remote had both moved on; what the
/// branch looks like after.
pub(super) fn pull(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 10.0;
    let each = (width - 2.0 * gap) / 3.0;
    let h = 140.0;
    let rect = area(ui, width, h + 26.0);
    let p = ui.painter_at(rect);
    let dot = |lane, label, new, merge| Dot {
        lane,
        label,
        new,
        merge,
    };
    for (i, mode) in PullMode::ALL.into_iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.left() + i as f32 * (each + gap), rect.top()),
            vec2(each, h),
        );
        p.rect_filled(r, t.radius, t.dock);
        let (name, rows): (&str, Vec<Dot>) = match mode {
            PullMode::Merge => (
                "Merge",
                vec![
                    dot(0, "merge", false, true),
                    dot(1, "theirs", false, false),
                    dot(0, "yours", false, false),
                    dot(0, "", false, false),
                ],
            ),
            PullMode::FastForwardOnly => (
                "Fast-forward only",
                vec![
                    dot(1, "theirs", false, false),
                    dot(0, "yours", false, false),
                    dot(0, "", false, false),
                ],
            ),
            PullMode::Rebase => (
                "Rebase",
                vec![
                    dot(0, "yours, anew", true, false),
                    dot(0, "theirs", false, false),
                    dot(0, "", false, false),
                ],
            ),
        };
        let pitch = 28.0;
        let x0 = r.left() + 18.0;
        let lane_x = |lane: usize| x0 + lane as f32 * 16.0;
        let y = |row: usize| r.top() + 22.0 + row as f32 * pitch;
        let last = rows.len() - 1;
        // Lines first: each dot to the one below in its lane, and the forks.
        for (row, d) in rows.iter().enumerate().take(last) {
            let below = rows[row + 1..]
                .iter()
                .position(|b| b.lane == d.lane)
                .map(|k| row + 1 + k)
                .unwrap_or(last);
            let color = t.lane(d.lane);
            if rows[below].lane == d.lane {
                p.line_segment(
                    [pos2(lane_x(d.lane), y(row)), pos2(lane_x(d.lane), y(below))],
                    Stroke::new(2.0, color),
                );
            }
            if d.lane == 1 {
                // A remote's commit forks off the shared one at the bottom.
                crate::repo::graph_view::edge(
                    &p,
                    pos2(lane_x(1), y(row)),
                    pos2(lane_x(0), y(last)),
                    color,
                    2.0,
                );
            }
            if d.merge {
                crate::repo::graph_view::edge(
                    &p,
                    pos2(lane_x(0), y(row)),
                    pos2(lane_x(1), y(row + 1)),
                    t.lane(1),
                    2.0,
                );
            }
        }
        for (row, d) in rows.iter().enumerate() {
            let c = pos2(lane_x(d.lane), y(row));
            let color = t.lane(d.lane);
            if d.new {
                p.circle_filled(c, 5.5, t.dock);
                p.circle_stroke(c, 5.0, Stroke::new(2.0, color));
            } else {
                p.circle_filled(c, if d.merge { 3.5 } else { 5.5 }, color);
            }
            draw::text(
                &p,
                pos2(lane_x(2) + 4.0, c.y),
                Align2::LEFT_CENTER,
                d.label,
                FontId::proportional(11.0),
                t.text_dim,
                r.right() - lane_x(2) - 8.0,
            );
        }
        // Where the branch ends up, or why it doesn't move.
        let note_y = r.bottom() - 14.0;
        let (note, color) = match mode {
            PullMode::Merge => ("A merge joins them", t.text_faint),
            PullMode::FastForwardOnly => ("Stops, changes nothing", t.warning),
            PullMode::Rebase => ("Yours, on top of theirs", t.text_faint),
        };
        draw::text(
            &p,
            pos2(r.left() + 8.0, note_y),
            Align2::LEFT_CENTER,
            note,
            FontId::proportional(10.5),
            color,
            r.width() - 14.0,
        );
        draw::choice(&p, t, r, name, s.pull_mode == mode);
    }
    ui.add_space(4.0);
    super::super::layout::note(
        ui,
        "When only the remote moved on, every way just fast-forwards your branch to it.",
    );
}

/// Code in diffs and blame: a few lines of Rust as a diff colours them.
pub(super) fn code(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let lines = [
        "/// Paints the app with `t` from now on.",
        "pub fn apply(ctx: &Context, id: &ThemeId) {",
        "    let t = resolve(id).tokens();",
        "    let t = resolve(id).tokens(&tweaks);",
        "    ctx.set_visuals(visuals(&t)); // 42 colours",
        "}",
    ];
    let kinds = [' ', ' ', '-', '+', ' ', ' '];
    let spans = crate::repo::highlight::highlight("preview.rs", lines.iter().copied(), t.dark);
    let line_h = 18.0;
    let rect = area(ui, width, lines.len() as f32 * line_h + 12.0);
    let p = ui.painter_at(rect);
    p.rect_filled(rect, t.radius, t.field);
    let font = theme::mono(11.5);
    for (i, line) in lines.iter().enumerate() {
        let y = rect.top() + 6.0 + line_h * (i as f32 + 0.5);
        let band = Rect::from_min_size(
            pos2(rect.left(), y - line_h / 2.0),
            vec2(rect.width(), line_h),
        );
        let (bg, sign) = match kinds[i] {
            '-' => (Some(t.diff_del_bg), "-"),
            '+' => (Some(t.diff_add_bg), "+"),
            _ => (None, " "),
        };
        if let Some(bg) = bg {
            p.rect_filled(band, 0.0, bg);
        }
        p.text(
            pos2(rect.left() + 8.0, y),
            Align2::LEFT_CENTER,
            format!("{}", i + 12),
            theme::mono(10.5),
            t.text_faint,
        );
        p.text(
            pos2(rect.left() + 32.0, y),
            Align2::LEFT_CENTER,
            sign,
            font.clone(),
            t.text_faint,
        );
        let mut x = rect.left() + 44.0;
        let clip = p.with_clip_rect(band.shrink2(vec2(4.0, 0.0)));
        match spans.as_ref().and_then(|s| s.get(i)) {
            Some(parts) => {
                for (color, text) in parts {
                    let r = clip.text(pos2(x, y), Align2::LEFT_CENTER, text, font.clone(), *color);
                    x = r.right();
                }
            }
            None => {
                clip.text(pos2(x, y), Align2::LEFT_CENTER, *line, font.clone(), t.text);
            }
        }
    }
    ui.add_space(6.0);
    let note = if !s.vscode_colors {
        "gitcraft's own colours for code".to_owned()
    } else {
        match crate::vscode_theme::current() {
            Some(found) if found.dark == t.dark => format!("Coloured as {} does", found.name),
            Some(found) => format!(
                "{} is {}: gitcraft's colours meanwhile",
                found.name,
                if found.dark { "dark" } else { "light" }
            ),
            None => "No VS Code theme found: gitcraft's own colours".to_owned(),
        }
    };
    super::super::layout::note(ui, note);
}

/// Authors with their pictures, or with their initials.
pub(super) fn avatars(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 12.0;
    let each = (width - gap) / 2.0;
    let h = 96.0;
    let rect = area(ui, width, h + 26.0);
    let p = ui.painter_at(rect);
    let people = ["Ada Lovelace", "Grace Hopper", "Linus Torvalds"];
    for (i, (name, pictures)) in [("Initials", false), ("Pictures", true)]
        .into_iter()
        .enumerate()
    {
        let r = Rect::from_min_size(
            pos2(rect.left() + i as f32 * (each + gap), rect.top()),
            vec2(each, h),
        );
        p.rect_filled(r, t.radius, t.dock);
        for (k, who) in people.iter().enumerate() {
            let y = r.top() + 18.0 + k as f32 * 28.0;
            let c = pos2(r.left() + 20.0, y);
            if pictures {
                face(&p, t, c, 10.0, k);
            } else {
                p.circle_filled(c, 10.0, t.lane(name_hash(who)));
                p.text(
                    c,
                    Align2::CENTER_CENTER,
                    initials(who),
                    theme::semibold(8.0),
                    Color32::WHITE,
                );
            }
            draw::text(
                &p,
                pos2(c.x + 16.0, y),
                Align2::LEFT_CENTER,
                who,
                FontId::proportional(11.5),
                t.text_dim,
                r.right() - c.x - 22.0,
            );
        }
        draw::choice(&p, t, r, name, s.avatars == pictures);
    }
}

/// A made-up picture: a head and shoulders on a coloured ground.
fn face(p: &Painter, t: &Tokens, c: Pos2, radius: f32, k: usize) {
    let grounds = [t.lane(5), t.lane(3), t.lane(4)];
    p.circle_filled(
        c,
        radius,
        grounds[k % 3].lerp_to_gamma(Color32::WHITE, 0.35),
    );
    let skin = [
        Color32::from_rgb(241, 194, 160),
        Color32::from_rgb(198, 134, 96),
        Color32::from_rgb(141, 85, 54),
    ][k % 3];
    let clip = p.with_clip_rect(Rect::from_center_size(c, vec2(radius * 2.0, radius * 2.0)));
    clip.circle_filled(
        c + vec2(0.0, radius * 1.05),
        radius * 0.75,
        grounds[k % 3].lerp_to_gamma(Color32::BLACK, 0.3),
    );
    clip.circle_filled(c + vec2(0.0, -radius * 0.15), radius * 0.4, skin);
}

/// A few changed lines: old and new in one column, or side by side.
pub(super) fn diff_layout(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 12.0;
    let each = (width - gap) / 2.0;
    let h = 110.0;
    let rect = area(ui, width, h + 26.0);
    let p = ui.painter_at(rect);
    // Unified.
    let u = Rect::from_min_size(rect.min, vec2(each, h));
    p.rect_filled(u, t.radius, t.field);
    let rows = [
        (' ', 0.7),
        ('-', 0.55),
        ('-', 0.4),
        ('+', 0.6),
        ('+', 0.45),
        (' ', 0.5),
    ];
    for (i, (kind, w)) in rows.iter().enumerate() {
        diff_line(&p, t, u, i, *kind, *w);
    }
    draw::choice(&p, t, u, "Unified", !s.diff_split);
    // Side by side: the old lines left, the new right.
    let sx = Rect::from_min_size(pos2(rect.left() + each + gap, rect.top()), vec2(each, h));
    p.rect_filled(sx, t.radius, t.field);
    let half = (sx.width() - 1.0) / 2.0;
    let left = Rect::from_min_size(sx.min, vec2(half, h));
    let right = Rect::from_min_size(pos2(sx.left() + half + 1.0, sx.top()), vec2(half, h));
    p.line_segment(
        [
            pos2(left.right() + 0.5, sx.top()),
            pos2(left.right() + 0.5, sx.bottom()),
        ],
        Stroke::new(1.0, t.separator),
    );
    for (i, (l, r, w)) in [
        (' ', ' ', 0.7),
        ('-', '+', 0.6),
        ('-', '+', 0.45),
        (' ', ' ', 0.5),
    ]
    .iter()
    .enumerate()
    {
        diff_line(&p, t, left, i, *l, *w);
        diff_line(&p, t, right, i, *r, *w * 0.9);
    }
    draw::choice(&p, t, sx, "Side by side", s.diff_split);
}

/// Line `i` of a diff drawn in `r`: its number, sign, background and a bar for its text.
fn diff_line(p: &Painter, t: &Tokens, r: Rect, i: usize, kind: char, w: f32) {
    let h = 16.0;
    let y = r.top() + 8.0 + h * (i as f32 + 0.5);
    let band = Rect::from_min_size(pos2(r.left(), y - h / 2.0), vec2(r.width(), h));
    let (bg, fg) = match kind {
        '-' => (Some(t.diff_del_bg), t.danger),
        '+' => (Some(t.diff_add_bg), t.success),
        '@' => (Some(t.accent.gamma_multiply(0.12)), t.accent),
        _ => (None, t.text_dim),
    };
    if let Some(bg) = bg {
        p.rect_filled(band, 0.0, bg);
    }
    p.text(
        pos2(r.left() + 6.0, y),
        Align2::LEFT_CENTER,
        format!("{}", 40 + i),
        theme::mono(9.5),
        t.text_faint,
    );
    if kind == '@' {
        p.text(
            pos2(r.left() + 26.0, y),
            Align2::LEFT_CENTER,
            "@@ -52,6 +52,7 @@",
            theme::mono(9.5),
            fg,
        );
        return;
    }
    if kind == '-' || kind == '+' {
        p.text(
            pos2(r.left() + 26.0, y),
            Align2::LEFT_CENTER,
            kind.to_string(),
            theme::mono(10.0),
            fg,
        );
    }
    let x = r.left() + 36.0;
    draw::bar(
        p,
        x,
        y,
        (r.right() - x - 6.0) * w,
        4.0,
        if kind == ' ' {
            t.text_faint
        } else {
            fg.gamma_multiply(0.8)
        },
    );
}

/// The lines around the changes, with what lies between folded; or the whole file.
pub(super) fn diff_context(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 12.0;
    let each = (width - gap) / 2.0;
    let h = 150.0;
    let rect = area(ui, width, h + 26.0);
    let p = ui.painter_at(rect);
    let around = Rect::from_min_size(rect.min, vec2(each, h));
    p.rect_filled(around, t.radius, t.field);
    let rows = [('@', 0.0), (' ', 0.6), ('-', 0.5), ('+', 0.55), (' ', 0.4)];
    for (i, (k, w)) in rows.iter().enumerate() {
        diff_line(&p, t, around, i, *k, *w);
    }
    let fold_y = around.top() + 8.0 + 16.0 * 5.0 + 10.0;
    p.text(
        pos2(around.center().x, fold_y),
        Align2::CENTER_CENTER,
        "… 120 lines unchanged",
        FontId::proportional(10.5),
        t.text_faint,
    );
    draw::choice(&p, t, around, "Around the changes", !s.diff_whole_file);
    let whole = Rect::from_min_size(pos2(rect.left() + each + gap, rect.top()), vec2(each, h));
    p.rect_filled(whole, t.radius, t.field);
    let rows = [
        (' ', 0.5),
        (' ', 0.7),
        (' ', 0.35),
        (' ', 0.6),
        ('-', 0.5),
        ('+', 0.55),
        (' ', 0.4),
        (' ', 0.65),
    ];
    for (i, (k, w)) in rows.iter().enumerate() {
        diff_line(&p, t, whole, i, *k, *w);
    }
    draw::choice(&p, t, whole, "The whole file", s.diff_whole_file);
}

/// The changed files in their folders, or in one list.
pub(super) fn file_list(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 12.0;
    let each = (width - gap) / 2.0;
    let h = 132.0;
    let rect = area(ui, width, h + 26.0);
    let p = ui.painter_at(rect);
    let row_h = 20.0;
    let tree = Rect::from_min_size(rect.min, vec2(each, h));
    p.rect_filled(tree, t.radius, t.card);
    let entries: [(usize, &str, Option<char>); 6] = [
        (0, "src", None),
        (1, "theme", None),
        (2, "catalog.rs", Some('A')),
        (2, "mod.rs", Some('M')),
        (1, "settings.rs", Some('M')),
        (0, "README.md", Some('M')),
    ];
    for (i, (depth, name, status)) in entries.iter().enumerate() {
        let y = tree.top() + 8.0 + row_h * (i as f32 + 0.5);
        let x = tree.left() + 10.0 + *depth as f32 * 14.0;
        file_entry(&p, t, tree, x, y, name, *status);
    }
    draw::choice(&p, t, tree, "In folders", s.files_as_tree);
    let list = Rect::from_min_size(pos2(rect.left() + each + gap, rect.top()), vec2(each, h));
    p.rect_filled(list, t.radius, t.card);
    let files: [(&str, &str, char); 4] = [
        ("catalog.rs", "src/theme", 'A'),
        ("mod.rs", "src/theme", 'M'),
        ("settings.rs", "src", 'M'),
        ("README.md", "", 'M'),
    ];
    for (i, (name, folder, status)) in files.iter().enumerate() {
        let y = list.top() + 8.0 + row_h * (i as f32 + 0.5);
        let r = file_entry(&p, t, list, list.left() + 10.0, y, name, Some(*status));
        if !folder.is_empty() {
            draw::text(
                &p,
                pos2(r.right() + 6.0, y),
                Align2::LEFT_CENTER,
                folder,
                FontId::proportional(10.5),
                t.text_faint,
                list.right() - r.right() - 30.0,
            );
        }
    }
    draw::choice(&p, t, list, "One list", !s.files_as_tree);
}

/// A file or folder of a changed-files list; returns the rect of its name.
fn file_entry(
    p: &Painter,
    t: &Tokens,
    area: Rect,
    x: f32,
    y: f32,
    name: &str,
    status: Option<char>,
) -> Rect {
    let font = FontId::proportional(11.5);
    match status {
        None => {
            // A folder: an open chevron.
            p.add(egui::Shape::line(
                vec![
                    pos2(x - 3.0, y - 2.0),
                    pos2(x, y + 1.5),
                    pos2(x + 3.0, y - 2.0),
                ],
                Stroke::new(1.3, t.text_faint),
            ));
            draw::text(
                p,
                pos2(x + 8.0, y),
                Align2::LEFT_CENTER,
                name,
                font,
                t.text_dim,
                area.right() - x - 14.0,
            )
        }
        Some(code) => {
            let color = crate::repo::widgets::status_color(t, code);
            p.text(
                pos2(area.right() - 12.0, y),
                Align2::CENTER_CENTER,
                code.to_string(),
                theme::semibold(10.5),
                color,
            );
            draw::text(
                p,
                pos2(x, y),
                Align2::LEFT_CENTER,
                name,
                font,
                t.text,
                area.right() - x - 30.0,
            )
        }
    }
}
