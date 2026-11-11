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

mod display;
pub(crate) use display::{credit as display_credit, diagnostic as display_diagnostic};

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
                name: capped(name),
                credit: capped(credit),
                palette,
            }),
            Err(e) => errors.push((file, e)),
        }
    }
    (themes, errors)
}

/// What the theme files in `dir` are now: their names, sizes and times, hashed. It changes
/// when any of them does, whatever the times say (a file from the future included).
pub fn fingerprint(dir: &Path) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let mut entries: Vec<(String, u64, Option<std::time::SystemTime>)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| kind_of(&e.path()).is_some())
        .map(|e| {
            let meta = e.metadata().ok();
            (
                e.file_name().to_string_lossy().into_owned(),
                meta.as_ref().map_or(0, |m| m.len()),
                meta.and_then(|m| m.modified().ok()),
            )
        })
        .collect();
    entries.sort();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    entries.hash(&mut h);
    Some(h.finish())
}

/// Longest a theme's name is kept: a file may say anything.
const MAX_NAME: usize = 80;

fn capped(name: String) -> String {
    if name.chars().count() <= MAX_NAME {
        return name;
    }
    name.chars().take(MAX_NAME - 1).collect::<String>() + "…"
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

mod formats;
pub use formats::*;

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
    let dir = dir().ok_or("no folder to keep themes in")?;
    import_into(from, &dir)
}

/// [`import`] into `dir`.
fn import_into(from: &Path, dir: &Path) -> Result<String, String> {
    let (name, _, _) = load(from)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Already one of the themes there: nothing to copy.
    let canonical = |p: &Path| std::fs::canonicalize(p).ok();
    if from
        .parent()
        .and_then(canonical)
        .is_some_and(|p| Some(p) == canonical(dir))
    {
        return Ok(file_name(from));
    }
    let dir = dir.to_owned();
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
            // So it reads back as a VS Code theme even when its colours are few.
            doc.insert("tokenColors".into(), json!([]));
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
mod tests;
