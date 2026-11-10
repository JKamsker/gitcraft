//! Decode native, VS Code, and Base16 theme palettes.
use super::*;

/// The palette of the theme called or saved as `name`, among those that come with gitcraft.
pub fn builtin_palette(name: &str) -> Option<Palette> {
    let lower = name.trim().to_lowercase();
    BUILTIN
        .iter()
        .find(|b| b.id.to_lowercase() == lower || b.name.to_lowercase() == lower)
        .map(|b| b.palette)
}

/// gitcraft's own theme file.
pub(super) fn from_own(json: &Value) -> Result<(Option<String>, Palette), String> {
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
pub(super) fn colours<const N: usize>(
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
    p.accent_fg = first(&["textLink.foreground"]).map(|c| over(c, p.chrome));
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
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches('"');
        let value = yaml_scalar(value);
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

/// A YAML scalar as the schemes write them: quoted (`"Material #2"`, a `#` inside kept), or
/// plain with an optional ` # comment` after it.
pub(super) fn yaml_scalar(value: &str) -> &str {
    let value = value.trim();
    for quote in ['"', '\''] {
        if let Some(rest) = value.strip_prefix(quote) {
            return rest.find(quote).map_or(rest, |end| &rest[..end]).trim();
        }
    }
    value.split(" #").next().unwrap_or(value).trim()
}
