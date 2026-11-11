use super::*;
use crate::theme::palette::hex;

fn temp_dir(what: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-themes-{what}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
fn reads_a_classic_base16_scheme() {
    let text = r#"
scheme: "Ocean"
author: "Chris Kempson (http://chriskempson.com)"
base00: "2b303b"
base01: "343d46"
base02: "4f5b66"
base03: "65737e"
base04: "a7adba"
base05: "c0c5ce"
base06: "dfe1e8"
base07: "eff1f5"
base08: "bf616a"
base09: "d08770"
base0A: "ebcb8b"
base0B: "a3be8c"
base0C: "96b5b4"
base0D: "8fa1b3"
base0E: "b48ead"
base0F: "ab7967"
"#;
    let read = from_base16(text);
    assert!(read.is_some(), "not read as a scheme");
    let Some((name, author, p)) = read else {
        return;
    };
    assert_eq!(name.as_deref(), Some("Ocean"));
    assert!(author.is_some_and(|a| a.starts_with("Chris")));
    assert!(p.dark);
    assert_eq!(p.dock, hex(0x2b303b));
    assert_eq!(p.chrome, hex(0x343d46));
    assert_eq!(p.text, hex(0xc0c5ce));
    assert_eq!(p.accent, hex(0x8fa1b3));
    assert_eq!(p.red, hex(0xbf616a));
}

#[test]
fn reads_a_tinted_theming_scheme() {
    let text = "system: \"base16\"\nname: \"Paper\"\nvariant: \"light\"\npalette:\n  \
                    base00: \"#f2eede\"\n  base01: \"#e8e4d4\"\n  base02: \"#d8d4c4\"\n  \
                    base03: \"#888888\"\n  base04: \"#555555\"\n  base05: \"#222222\"\n  \
                    base06: \"#111111\"\n  base07: \"#000000\"\n  base08: \"#cc3e28\"\n  \
                    base09: \"#d26f00\"\n  base0A: \"#b58900\"\n  base0B: \"#216609\"\n  \
                    base0C: \"#158c86\"\n  base0D: \"#1e6fcc\"\n  base0E: \"#5c21a5\"\n  \
                    base0F: \"#8b4513\"\n";
    let read = from_base16(text);
    assert!(read.is_some(), "not read as a scheme");
    let Some((name, _, p)) = read else {
        return;
    };
    assert_eq!(name.as_deref(), Some("Paper"));
    assert!(!p.dark);
    assert_eq!(p.dock, hex(0xf2eede));
}

#[test]
fn a_scheme_missing_colours_is_none() {
    assert!(from_base16("scheme: x\nbase00: \"000000\"\n").is_none());
}

#[test]
fn maps_vscode_workbench_colours() {
    let colors = json!({
        "editor.background": "#1e1e1e",
        "sideBar.background": "#252526",
        "titleBar.activeBackground": "#3c3c3c",
        "foreground": "#cccccc",
        "button.background": "#0e639c",
        "button.foreground": "#ffffff",
        "list.activeSelectionBackground": "#04395e",
        "terminal.ansiRed": "#cd3131",
        "list.hoverBackground": "#ffffff1a",
    });
    let p = from_vscode(colors.as_object().unwrap_or(&Map::new()), None);
    assert!(p.dark);
    assert_eq!(p.dock, hex(0x1e1e1e));
    assert_eq!(p.chrome, hex(0x252526));
    assert_eq!(p.tab_strip, hex(0x3c3c3c));
    assert_eq!(p.text, hex(0xcccccc));
    assert_eq!(p.accent, hex(0x0e639c));
    assert_eq!(p.red, hex(0xcd3131));
    assert_eq!(p.row_selected, Some(hex(0x04395e)));
    // A translucent hover, laid over the side bar.
    let hover = p.hover.unwrap_or_default();
    assert!(hover.r() > 0x25 && hover.r() < 0x40 && hover.a() == 255);
}

#[test]
fn own_files_extend_a_theme_and_set_any_colour() {
    let json = json!({
        "name": "Mine",
        "extends": "dracula",
        "colors": { "accent": "#ff0000", "hover": "#123456" },
        "lanes": ["#ff0000", "#00ff00"],
    });
    let read = from_own(&json);
    assert!(read.is_ok(), "not read as a theme");
    let Ok((name, p)) = read else {
        return;
    };
    assert_eq!(name.as_deref(), Some("Mine"));
    assert_eq!(p.accent, hex(0xff0000));
    assert_eq!(p.dock, hex(0x282a36));
    let t = p.tokens();
    assert_eq!(t.hover, hex(0x123456));
    assert_eq!(t.lanes[2], hex(0xff0000));
    assert_eq!(t.lanes[3], hex(0x00ff00));
    let bad = json!({ "colors": { "nope": "#000000" } });
    assert!(from_own(&bad).is_err_and(|e| e.contains("nope")));
    let bad = json!({ "extends": "no such theme" });
    assert!(from_own(&bad).is_err());
}

#[test]
fn a_saved_copy_reads_back_as_the_same_theme() {
    let dir = temp_dir("copy");
    let read = builtin_palette("Catppuccin Mocha");
    assert!(read.is_some(), "not in the catalogue");
    let Some(palette) = read else {
        return;
    };
    let path = dir.join("copy.json");
    let _ = std::fs::write(
        &path,
        to_own_json("Copy", &palette, Some("catppuccin-mocha")),
    );
    let read = load(&path);
    let (themes, errors) = scan(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(read.is_ok(), "the copy does not read back");
    let Ok((name, _, back)) = read else {
        return;
    };
    assert_eq!(name, "Copy");
    assert_eq!(back.tokens(), palette.tokens());
    assert_eq!(themes.len(), 1);
    assert!(errors.is_empty());
}

#[test]
fn scan_reports_files_it_cannot_read() {
    let dir = temp_dir("scan");
    let _ = std::fs::write(dir.join("broken.json"), "{ not json");
    let _ = std::fs::write(dir.join("notes.txt"), "not a theme");
    let (themes, errors) = scan(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(themes.is_empty());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, "broken.json");
}

#[test]
fn quoted_scheme_values_keep_their_hash() {
    assert_eq!(yaml_scalar(r#" "Material #2" # a comment"#), "Material #2");
    assert_eq!(yaml_scalar(" '#181818'"), "#181818");
    assert_eq!(yaml_scalar(" 181818 # plain, then a comment"), "181818");
}

#[test]
fn long_names_are_cut() {
    let name = capped("x".repeat(5000));
    assert_eq!(name.chars().count(), MAX_NAME);
    assert!(name.ends_with('…'));
    assert_eq!(capped("Short".to_owned()), "Short");
}

#[test]
fn importing_a_theme_already_there_copies_nothing() {
    let dir = temp_dir("import-same");
    let path = dir.join("mine.json");
    let _ = std::fs::write(&path, r#"{ "name": "Mine", "dark": true }"#);
    let back = import_into(&path, &dir);
    let count = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(back.ok().as_deref(), Some("mine.json"));
    assert_eq!(count, 1);
}

#[test]
fn an_imported_vscode_theme_with_an_include_reads_back_as_one() {
    let from = temp_dir("import-from");
    let to = temp_dir("import-to");
    let _ = std::fs::write(
        from.join("base.json"),
        r##"{ "colors": { "editor.background": "#fafafa" } }"##,
    );
    let _ = std::fs::write(
        from.join("light.json"),
        r##"{ "name": "Pale", "type": "light", "include": "./base.json" }"##,
    );
    let file = import_into(&from.join("light.json"), &to);
    let read = file.as_ref().ok().map(|f| load(&to.join(f)));
    let _ = std::fs::remove_dir_all(&from);
    let _ = std::fs::remove_dir_all(&to);
    assert!(matches!(read, Some(Ok(_))), "the import does not read back");
    let Some(Ok((name, credit, palette))) = read else {
        return;
    };
    assert_eq!(name, "Pale");
    assert!(credit.starts_with("VS Code"));
    assert!(!palette.dark);
    assert_eq!(palette.dock, hex(0xfafafa));
}

#[test]
fn the_fingerprint_follows_any_change() {
    let dir = temp_dir("fingerprint");
    let path = dir.join("a.json");
    let _ = std::fs::write(&path, "{}");
    let first = fingerprint(&dir);
    // Same time (set back by hand), other contents: still a change.
    let when = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let _ = std::fs::write(&path, r#"{ "name": "B" }"#);
    if let (Some(when), Ok(file)) = (when, std::fs::File::options().write(true).open(&path)) {
        let _ = file.set_modified(when);
    }
    let second = fingerprint(&dir);
    let _ = std::fs::write(dir.join("notes.txt"), "not a theme");
    let third = fingerprint(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(first.is_some());
    assert_ne!(first, second);
    // Files that aren't themes don't count.
    assert_eq!(second, third);
}
