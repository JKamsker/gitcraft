use super::*;
use crate::theme::palette::contrast;

#[test]
fn ids_are_unique() {
    let mut ids: Vec<_> = BUILTIN.iter().map(|b| b.id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n);
}

#[test]
fn every_builtin_small_text_token_meets_wcag_aa_on_its_surfaces() {
    let mut unreadable = Vec::new();
    for b in BUILTIN {
        let t = b.palette().tokens();
        let mut check = |what: &str, fg, surface: &str, bg| {
            let ratio = contrast(fg, bg);
            if ratio < 4.5 {
                unreadable.push(format!("{}: {what} on {surface} is {ratio:.2}:1", b.name));
            }
        };
        let surfaces = [
            ("tab strip", t.tab_strip),
            ("dock", t.dock),
            ("card", t.card),
            ("field", t.field),
            ("hover", t.hover),
            ("pressed", t.pressed),
            ("selected row", t.row_selected),
        ];
        for (name, fg) in [
            ("text", t.text),
            ("dim text", t.text_dim),
            ("faint text", t.text_faint),
            ("accent foreground", t.accent_fg),
            ("danger", t.danger),
            ("warning", t.warning),
            ("success", t.success),
        ] {
            for (surface, bg) in surfaces {
                if name == "text" || name == "dim text" || surface != "pressed" {
                    check(name, fg, surface, bg);
                }
            }
        }
        for (name, fg) in [
            ("accent chip", t.accent_fg),
            ("danger chip", t.danger),
            ("warning chip", t.warning),
            ("success chip", t.success),
        ] {
            for (surface, bg) in surfaces.iter().filter(|(name, _)| *name != "pressed") {
                for opacity in [0.18, 0.22] {
                    check(name, fg, surface, over(t.text_tint(fg, opacity, *bg), *bg));
                }
            }
        }
        check("button label", t.accent_text, "accent", t.accent);
        check(
            "diff hunk label",
            t.accent_fg,
            "hunk fill",
            over(t.accent.gamma_multiply(0.12), t.field),
        );
        for (name, fill) in [
            ("added line", t.diff_add_bg),
            ("deleted line", t.diff_del_bg),
        ] {
            check("diff text", t.text, name, over(fill, t.field));
            let added = name == "added line";
            for surface in [t.field, t.card] {
                let line = over(fill, surface);
                for bg in [line, over(t.accent.gamma_multiply(0.22), line)] {
                    check(
                        "intraline text",
                        t.text,
                        name,
                        over(t.diff_strong_bg(added), bg),
                    );
                }
            }
        }
    }
    assert!(unreadable.is_empty(), "{unreadable:#?}");
}

#[test]
fn contrast_changes_preserve_surfaces_terminal_and_graph_colours() {
    for b in BUILTIN {
        let original = b.palette.tokens();
        let adjusted = b.palette().tokens();
        assert_eq!(adjusted.tab_strip, original.tab_strip, "{}", b.name);
        assert_eq!(adjusted.dock, original.dock, "{}", b.name);
        assert_eq!(adjusted.chrome, original.chrome, "{}", b.name);
        assert_eq!(adjusted.field, original.field, "{}", b.name);
        assert_eq!(adjusted.hover, original.hover, "{}", b.name);
        assert_eq!(adjusted.pressed, original.pressed, "{}", b.name);
        assert_eq!(adjusted.row_selected, original.row_selected, "{}", b.name);
        assert_eq!(adjusted.lanes, original.lanes, "{}", b.name);
        assert_eq!(adjusted.ansi, original.ansi, "{}", b.name);
    }
    for b in &BUILTIN[..3] {
        let t = b.palette().tokens();
        assert_eq!(t.accent_text, Color32::WHITE);
        assert!(contrast(t.accent_text, t.accent) >= 4.5);
    }
}

#[test]
fn a_custom_palette_keeps_its_chosen_text_colours() {
    let mut p = BUILTIN[0].palette();
    p.text_faint = p.chrome;
    p.accent_fg = None;
    let t = p.tokens();
    assert_eq!(t.text_faint, p.chrome);
    assert_eq!(t.accent_fg, p.accent);
}

#[test]
fn the_pro_themes_keep_their_colours() {
    let dark = BUILTIN[0].palette().tokens();
    assert_eq!(dark.chrome, Color32::from_rgb(50, 50, 50));
    assert_eq!(dark.hover, Color32::from_rgb(66, 66, 66));
    assert_eq!(dark.row_selected, Color32::from_rgb(38, 76, 122));
    assert_eq!(dark.lanes, CLASSIC_DARK_LANES);
    let light = BUILTIN[2].palette().tokens();
    assert!(!light.dark);
    assert_eq!(light.field, Color32::WHITE);
    assert_eq!(light.separator, Color32::from_rgb(214, 214, 214));
}
