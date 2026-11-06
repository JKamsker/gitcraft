//! Themes of your own: files in gitcraft's `themes` folder, read again when they change. Three
//! kinds are understood, so themes found on the internet can be used as they come:
//!
//! - gitcraft's own JSON: a name, whether it is dark, a theme it starts from, and any colours
//!   by their keys (see [`super::palette::KEYS`]), the graph's `lanes` and the terminal's `ansi`.
//! - VS Code colour themes (`.json`, e.g. from an extension's `themes` folder): the workbench
//!   colours become the interface's.
//! - Base16 and Base24 schemes (`.yaml`), from the tinted-theming collection or anywhere else.

use std::path::{Path, PathBuf};

use egui::Color32;
use serde_json::{Map, Value, json};

use super::catalog::BUILTIN;
use super::palette::{self, KEYS, Palette, luminance, over, parse_hex, to_hex};

/// gitcraft's themes folder.
pub fn dir() -> Option<PathBuf> {
    eframe::storage_dir("gitcraft").map(|d| d.join("themes"))
}

/// A theme read from a file.
#[derive(Clone, Debug)]
pub struct FileTheme {
    /// The file's name in the themes folder.
    pub file: String,
    pub name: String,
    /// What kind of file it is, and who made it if it says.
    pub credit: String,
    pub palette: Palette,
}

/// The themes in `dir`, and the files that could not be read (their name, and why).
pub fn scan(dir: &Path) -> (Vec<FileTheme>, Vec<(String, String)>) {
    let mut themes = Vec::new();
    let mut errors = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (themes, errors);
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && kind_of(p).is_some())
        .collect();
    paths.sort();
    for path in paths {
        let file = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        match load(&path) {
            Ok((name, credit, palette)) => themes.push(FileTheme {
                file,
                name,
                credit,
                palette,
            }),
            Err(e) => errors.push((file, e)),
        }
    }
    (themes, errors)
}

/// The latest time a theme file in `dir` (or the folder itself) changed.
pub fn changed_at(dir: &Path) -> Option<std::time::SystemTime> {
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut latest = modified(dir)?;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        if let Some(m) = modified(&e.path()) {
            latest = latest.max(m);
        }
    }
    Some(latest)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Json,
    Yaml,
}

fn kind_of(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    match ext.as_str() {
        "json" | "jsonc" => Some(Kind::Json),
        "yaml" | "yml" => Some(Kind::Yaml),
        _ => None,
    }
}

/// A theme file's name, credit and palette.
pub fn load(path: &Path) -> Result<(String, String, Palette), String> {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    match kind_of(path) {
        Some(Kind::Yaml) => {
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            let (name, author, palette) =
                from_base16(&text).ok_or("not a Base16 or Base24 scheme")?;
            let name = name.unwrap_or(stem);
            let credit = match author {
                Some(a) => format!("Base16 scheme · {a}"),
                None => "Base16 scheme".to_owned(),
            };
            Ok((name, credit, palette))
        }
        Some(Kind::Json) => {
            let json = crate::vscode_theme::read_json(path).ok_or("not readable JSON")?;
            if is_vscode(&json) {
                let (colors, dark) =
                    crate::vscode_theme::read_workbench(path).ok_or("a broken VS Code theme")?;
                let name = json
                    .get("name")
                    .and_then(Value::as_str)
                    .map_or(stem, str::to_owned);
                let credit = match json.get("author").and_then(Value::as_str) {
                    Some(a) => format!("VS Code theme · {a}"),
                    None => "VS Code theme".to_owned(),
                };
                Ok((name, credit, from_vscode(&colors, dark)))
            } else {
                let (name, palette) = from_own(&json)?;
                let credit = json
                    .get("author")
                    .and_then(Value::as_str)
                    .map_or_else(|| "Your theme".to_owned(), |a| format!("Your theme · {a}"));
                Ok((name.unwrap_or(stem), credit, palette))
            }
        }
        None => Err("not a .json or .yaml file".to_owned()),
    }
}

/// A VS Code theme: workbench colours keyed like `editor.background`, or token colours.
fn is_vscode(json: &Value) -> bool {
    json.get("tokenColors").is_some()
        || json.get("include").is_some()
        || json
            .get("colors")
            .and_then(Value::as_object)
            .is_some_and(|c| c.keys().any(|k| k.contains('.')))
}

/// The palette of the theme called or saved as `name`, among those that come with gitcraft.
pub fn builtin_palette(name: &str) -> Option<Palette> {
    let lower = name.trim().to_lowercase();
    BUILTIN
        .iter()
        .find(|b| b.id.to_lowercase() == lower || b.name.to_lowercase() == lower)
        .map(|b| b.palette)
}

/// gitcraft's own theme file.
fn from_own(json: &Value) -> Result<(Option<String>, Palette), String> {
    let name = json.get("name").and_then(Value::as_str).map(str::to_owned);
    let dark = json.get("dark").and_then(Value::as_bool);
    let extends = json.get("extends").and_then(Value::as_str);
    let mut palette = match extends {
        Some(base) => {
            builtin_palette(base).ok_or_else(|| format!("no theme “{base}” to extend"))?
        }
        None => builtin_palette(if dark == Some(false) { "Light" } else { "Dark" })
            .ok_or("no base theme")?,
    };
    if let Some(dark) = dark {
        palette.dark = dark;
    }
    let colors = json.get("colors").and_then(Value::as_object);
    let mut unknown = Vec::new();
    for (key, value) in colors.into_iter().flatten() {
        let Some(c) = value.as_str().and_then(parse_hex) else {
            return Err(format!("“{key}” is not a colour like #1e1e2e"));
        };
        if !palette.set(key, c) {
            unknown.push(key.clone());
        }
    }
    if !unknown.is_empty() {
        return Err(format!(
            "no colour called {} (see the keys of a saved copy)",
            unknown.join(", ")
        ));
    }
    palette.lanes = colours(json.get("lanes"), "lanes")?.or(palette.lanes);
    palette.ansi = colours(json.get("ansi"), "ansi")?.or(palette.ansi);
    Ok((name, palette))
}

/// A list of `N` colours; none when `value` is missing.
fn colours<const N: usize>(
    value: Option<&Value>,
    what: &str,
) -> Result<Option<[Color32; N]>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let list: Vec<Color32> = value
        .as_array()
        .ok_or_else(|| format!("“{what}” is not a list of colours"))?
        .iter()
        .map(|v| v.as_str().and_then(parse_hex))
        .collect::<Option<_>>()
        .ok_or_else(|| format!("“{what}” has something that is not a colour"))?;
    if list.is_empty() {
        return Ok(None);
    }
    // Fewer lanes than ten repeat; the terminal needs all sixteen.
    if what == "ansi" && list.len() != N {
        return Err(format!("“{what}” needs {N} colours, has {}", list.len()));
    }
    let mut out = [Color32::BLACK; N];
    for (i, c) in out.iter_mut().enumerate() {
        *c = list[i % list.len()];
    }
    Ok(Some(out))
}

/// The interface's palette from a VS Code theme's workbench colours. `dark`: what the theme
/// says it is; otherwise its editor's background decides.
pub fn from_vscode(colors: &Map<String, Value>, dark: Option<bool>) -> Palette {
    let get = |key: &str| colors.get(key).and_then(Value::as_str).and_then(parse_hex);
    let first = |keys: &[&str]| keys.iter().find_map(|k| get(k));
    let dock_raw = first(&["editor.background"]);
    let dark = dark.unwrap_or_else(|| dock_raw.is_none_or(|c| luminance(c) < 0.4));
    let mut p = builtin_palette(if dark { "Dark" } else { "Light" }).unwrap_or(BUILTIN[0].palette);
    // Workbench colours may be translucent; laid over what they are on.
    let dock = dock_raw.map_or(p.dock, |c| over(c, p.dock));
    p.dock = dock;
    p.chrome = first(&[
        "sideBar.background",
        "panel.background",
        "editorGroupHeader.tabsBackground",
    ])
    .map_or(dock, |c| over(c, dock));
    p.tab_strip = first(&[
        "titleBar.activeBackground",
        "editorGroupHeader.tabsBackground",
        "activityBar.background",
    ])
    .map_or_else(
        || {
            p.chrome
                .lerp_to_gamma(Color32::BLACK, if dark { 0.2 } else { 0.06 })
        },
        |c| over(c, p.chrome),
    );
    p.field = first(&[
        "input.background",
        "dropdown.background",
        "editorWidget.background",
    ])
    .map_or(p.chrome, |c| over(c, p.chrome));
    let text = first(&["foreground", "sideBar.foreground", "editor.foreground"])
        .map(|c| over(c, p.chrome));
    if let Some(text) = text {
        p.text = text;
        p.text_dim = text.lerp_to_gamma(p.chrome, 0.3);
        p.text_faint = text.lerp_to_gamma(p.chrome, 0.5);
    }
    if let Some(c) = first(&["descriptionForeground"]) {
        p.text_dim = over(c, p.chrome);
    }
    if let Some(c) = first(&["disabledForeground", "editorLineNumber.foreground"]) {
        let c = over(c, p.chrome);
        // Some themes make it almost the background: keep hints readable.
        if palette::contrast(c, p.chrome) >= 2.2 {
            p.text_faint = c;
        }
    }
    p.border = first(&[
        "input.border",
        "panel.border",
        "editorGroup.border",
        "contrastBorder",
    ])
    .map_or_else(
        || p.chrome.lerp_to_gamma(p.text, 0.2),
        |c| over(c, p.chrome),
    );
    if let Some(c) = first(&["sideBar.border", "editorGroup.border"]) {
        p.separator = Some(over(c, p.chrome));
    } else {
        p.separator = None;
    }
    if let Some(c) = first(&["button.background", "focusBorder", "textLink.foreground"]) {
        p.accent = over(c, p.chrome);
        p.accent_text = first(&["button.foreground"]).map(|c| over(c, p.accent));
    }
    p.hover = first(&["list.hoverBackground"]).map(|c| over(c, p.chrome));
    p.pressed = None;
    p.icon = first(&["icon.foreground"]).map(|c| over(c, p.chrome));
    p.row_selected = first(&[
        "list.activeSelectionBackground",
        "editor.selectionBackground",
    ])
    .map(|c| over(c, dock));
    let ansi = |name: &str| get(&format!("terminal.ansi{name}"));
    let hue = |names: &[&str], fallback: Color32| {
        names
            .iter()
            .find_map(|n| ansi(n).or_else(|| get(n)))
            .map_or(fallback, |c| over(c, dock))
    };
    p.red = hue(&["Red", "errorForeground", "editorError.foreground"], p.red);
    p.green = hue(&["Green", "gitDecoration.addedResourceForeground"], p.green);
    p.yellow = hue(&["Yellow", "editorWarning.foreground"], p.yellow);
    p.blue = hue(&["Blue"], p.blue);
    p.magenta = hue(&["Magenta"], p.magenta);
    p.cyan = hue(&["Cyan"], p.cyan);
    p.orange = hue(&["gitDecoration.modifiedResourceForeground"], p.orange);
    p.danger = None;
    p.warning = None;
    p.success = None;
    p.diff_add_bg = first(&[
        "diffEditor.insertedLineBackground",
        "diffEditor.insertedTextBackground",
    ]);
    p.diff_del_bg = first(&[
        "diffEditor.removedLineBackground",
        "diffEditor.removedTextBackground",
    ]);
    p.lanes = None;
    const NAMES: [&str; 16] = [
        "Black",
        "Red",
        "Green",
        "Yellow",
        "Blue",
        "Magenta",
        "Cyan",
        "White",
        "BrightBlack",
        "BrightRed",
        "BrightGreen",
        "BrightYellow",
        "BrightBlue",
        "BrightMagenta",
        "BrightCyan",
        "BrightWhite",
    ];
    let all: Option<Vec<Color32>> = NAMES.iter().map(|n| ansi(n)).collect();
    p.ansi = all.map(|list| {
        let mut out = [Color32::BLACK; 16];
        for (o, c) in out.iter_mut().zip(list) {
            *o = over(c, dock);
        }
        out
    });
    p.dark = dark;
    p
}

/// A Base16 or Base24 scheme: its name, author and palette. Both the classic layout
/// (`base00: "181818"` at the top) and tinted-theming's (`palette:` with `base00: "#181818"`)
/// are read, without a YAML parser: the schemes are flat lists of keys.
pub fn from_base16(text: &str) -> Option<(Option<String>, Option<String>, Palette)> {
    let mut base = [None::<Color32>; 24];
    let mut name = None;
    let mut author = None;
    let mut variant = None;
    for line in text.lines() {
        let line = line.split(" #").next().unwrap_or(line).trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches('"');
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        match key {
            "scheme" | "name" if !value.is_empty() => name = Some(value.to_owned()),
            "author" if !value.is_empty() => author = Some(value.to_owned()),
            "variant" => variant = Some(value.to_lowercase()),
            _ => {
                if let Some(n) = key
                    .strip_prefix("base")
                    .and_then(|n| usize::from_str_radix(n, 16).ok())
                    .filter(|&n| n < 24)
                {
                    base[n] = parse_hex(value);
                }
            }
        }
    }
    let b: Vec<Color32> = base[..16].iter().copied().collect::<Option<_>>()?;
    let dark = match variant.as_deref() {
        Some("light") => false,
        Some("dark") => true,
        _ => luminance(b[0]) < 0.4,
    };
    let mix = |a: Color32, c: Color32, t: f32| a.lerp_to_gamma(c, t);
    // Base24 has darker backgrounds of its own.
    let darker = base[0x10].or(base[0x11]);
    let mut p = Palette::new(
        dark,
        [
            if dark {
                darker.unwrap_or_else(|| mix(b[0], Color32::BLACK, 0.3))
            } else {
                mix(b[1], b[2], 0.5)
            },
            b[0],
            b[1],
            if dark {
                mix(b[0], Color32::BLACK, 0.18)
            } else {
                mix(b[0], Color32::WHITE, 0.5)
            },
            mix(b[2], b[3], 0.4),
        ],
        [b[5], b[4], mix(b[3], b[4], 0.35)],
        b[0xd],
        [b[8], b[0xb], b[0xa], b[0xd], b[0xe], b[0xc], b[9]],
    );
    p.row_selected = Some(b[2]);
    if base[0x12..].iter().all(Option::is_some) {
        // Base24's bright colours make the terminal's.
        let bright = |i: usize| base[i].unwrap_or(Color32::BLACK);
        p.ansi = Some([
            b[1],
            b[8],
            b[0xb],
            b[0xa],
            b[0xd],
            b[0xe],
            b[0xc],
            b[5],
            b[3],
            bright(0x12),
            bright(0x14),
            bright(0x13),
            bright(0x16),
            bright(0x17),
            bright(0x15),
            b[7],
        ]);
    }
    Some((name, author, p))
}

/// A theme file of gitcraft's own with every colour of `palette`, starting from the theme
/// saved as `extends`: a copy to change.
pub fn to_own_json(name: &str, palette: &Palette, extends: Option<&str>) -> String {
    // Written by hand to keep the keys in the order they are read best in.
    let quote = |s: &str| json!(s).to_string();
    let mut out = String::from("{\n");
    out += &format!("  \"name\": {},\n", quote(name));
    out += &format!("  \"dark\": {},\n", palette.dark);
    if let Some(base) = extends {
        out += &format!("  \"extends\": {},\n", quote(base));
    }
    let colors: Vec<String> = KEYS
        .iter()
        .filter_map(|key| Some(format!("    \"{key}\": \"{}\"", to_hex(palette.get(key)?))))
        .collect();
    out += &format!("  \"colors\": {{\n{}\n  }},\n", colors.join(",\n"));
    let t = palette.tokens();
    let list = |cs: &[Color32]| {
        cs.iter()
            .map(|c| format!("\"{}\"", to_hex(*c)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    out += &format!("  \"lanes\": [{}],\n", list(&t.lanes));
    out += &format!("  \"ansi\": [{}]\n", list(&t.ansi));
    out += "}\n";
    out
}

/// A file name in `dir` from `name`, not taken yet.
fn free_name(dir: &Path, name: &str, ext: &str) -> PathBuf {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() {
        "theme".to_owned()
    } else {
        slug
    };
    let mut path = dir.join(format!("{slug}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{slug}-{n}.{ext}"));
        n += 1;
    }
    path
}

/// Saves a copy of `palette` to change, called `name`, into the themes folder; returns the
/// file's name there.
pub fn save_copy(name: &str, palette: &Palette, extends: Option<&str>) -> Result<String, String> {
    let dir = dir().ok_or("no folder to keep themes in")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = free_name(&dir, name, "json");
    std::fs::write(&path, to_own_json(name, palette, extends)).map_err(|e| e.to_string())?;
    Ok(file_name(&path))
}

/// Copies the theme file at `from` into the themes folder, once it reads as a theme; returns
/// its file's name there.
pub fn import(from: &Path) -> Result<String, String> {
    let (name, _, _) = load(from)?;
    let dir = dir().ok_or("no folder to keep themes in")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ext = from
        .extension()
        .map_or("json".to_owned(), |e| e.to_string_lossy().to_lowercase());
    // A VS Code theme may `include` another file beside it: that one is read now, and the
    // whole of it kept.
    let to = free_name(&dir, &name, &ext);
    match crate::vscode_theme::read_json(from) {
        Some(json) if is_vscode(&json) && json.get("include").is_some() => {
            let (colors, dark) =
                crate::vscode_theme::read_workbench(from).ok_or("a broken VS Code theme")?;
            let mut doc = Map::new();
            doc.insert("name".into(), json!(name));
            if let Some(dark) = dark {
                doc.insert("type".into(), json!(if dark { "dark" } else { "light" }));
            }
            doc.insert("colors".into(), Value::Object(colors));
            let text = serde_json::to_string_pretty(&Value::Object(doc)).unwrap_or_default();
            std::fs::write(&to, text).map_err(|e| e.to_string())?;
        }
        _ => {
            std::fs::copy(from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(file_name(&to))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::palette::hex;

    fn temp_dir(what: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-themes-{what}-{}", std::process::id()));
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
}
