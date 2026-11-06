//! Running from an AppImage on Linux: an entry in the user's applications menu that starts this
//! AppImage file (the .deb and .rpm packages install their own).

use std::path::{Path, PathBuf};

/// File name of the entry, apart from the `gitcraft.desktop` a package installs.
const ENTRY: &str = "gitcraft-appimage.desktop";
const ICON: &[u8] = include_bytes!("../assets/app-icon/gitcraft-256.png");

/// The AppImage file gitcraft runs from (its runtime sets `APPIMAGE`), on Linux.
pub fn appimage() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            // An empty or relative HOME would put the entry under the current folder.
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|h| h.is_absolute())
                .map(|h| h.join(".local/share"))
        })
}

fn entry_path(data: &Path) -> PathBuf {
    data.join("applications").join(ENTRY)
}

fn icon_path(data: &Path) -> PathBuf {
    data.join("icons/hicolor/256x256/apps/gitcraft.png")
}

/// Whether the menu entry exists.
pub fn installed() -> bool {
    data_home().is_some_and(|d| entry_path(&d).is_file())
}

/// Adds (or rewrites) the menu entry for `appimage`.
pub fn install(appimage: &Path) -> Result<(), String> {
    let data = data_home().ok_or("No home folder")?;
    write_entry(&data, appimage).map_err(|e| e.to_string())
}

/// Removes the menu entry and its icon.
pub fn uninstall() -> Result<(), String> {
    let data = data_home().ok_or("No home folder")?;
    for path in [entry_path(&data), icon_path(&data)] {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(())
}

/// At startup: an AppImage that was moved or replaced by a newer file keeps its menu entry
/// pointing at the file running now.
pub fn refresh() {
    let (Some(appimage), Some(data)) = (appimage(), data_home()) else {
        return;
    };
    let path = entry_path(&data);
    let Ok(current) = std::fs::read_to_string(&path) else {
        return;
    };
    // The actions (set later, from the recent repositories) aside.
    let head = |text: &str| text.split("Actions=").next().unwrap_or_default().to_owned();
    if head(&current) != head(&entry(&appimage)) {
        let _ = write_entry(&data, &appimage);
    }
}

/// The actions the entries list (the recent repositories, Clone…, Open…), as last set.
fn actions() -> Vec<crate::os_shell::MenuItem> {
    ACTIONS.lock().map(|a| a.clone()).unwrap_or_default()
}

static ACTIONS: std::sync::Mutex<Vec<crate::os_shell::MenuItem>> =
    std::sync::Mutex::new(Vec::new());

/// What marks the user's own copy of the package's entry as written by gitcraft (one written by
/// the user is never touched).
const OURS: &str = "X-Gitcraft-Actions=true";

/// The entry of the package, which a copy of the user's own takes the place of.
const PACKAGE_ENTRY: &str = "/usr/share/applications/gitcraft.desktop";

/// Puts `items` in the launcher's menu: the AppImage's entry, when it is in the applications
/// menu; for the .deb or .rpm, a copy of the package's entry in the user's applications folder
/// (which takes its place), unless the user has one of their own there.
pub fn set_actions(items: &[crate::os_shell::MenuItem]) {
    if let Ok(mut a) = ACTIONS.lock() {
        *a = items.to_vec();
    }
    let Some(data) = data_home() else {
        return;
    };
    if let Some(appimage) = appimage() {
        let path = entry_path(&data);
        if path.is_file() {
            let _ = write_whole(&path, &entry(&appimage));
        }
        return;
    }
    if !cfg!(target_os = "linux") {
        return;
    }
    let path = data.join("applications/gitcraft.desktop");
    // Ours: one marked so, or none yet; anything else there (even unreadable) is the user's.
    let ours = match std::fs::read_to_string(&path) {
        Ok(text) => text.contains(OURS),
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    };
    if !ours {
        return;
    }
    if Path::new(PACKAGE_ENTRY).is_file() {
        let _ = std::fs::create_dir_all(data.join("applications"));
        let _ = write_whole(&path, &package_entry(items));
    } else {
        // The package is gone: so goes its copy.
        let _ = std::fs::remove_file(&path);
    }
}

/// The desktop file the window belongs to, by its id: the AppImage's when its entry is in the
/// menu, else the package's (or the user's copy of it, which has the same id).
pub fn desktop_id() -> &'static str {
    if appimage().is_some() && installed() {
        ENTRY
    } else {
        "gitcraft.desktop"
    }
}

/// Writes `text` to `path` all at once (through a file beside it, renamed over it), so that a
/// menu reading it meanwhile never sees half of it.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let part = path.with_extension(format!("desktop.{}.part", std::process::id()));
    std::fs::write(&part, text)?;
    std::fs::rename(&part, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&part);
    })
}

/// The package's entry with `items` as its actions.
fn package_entry(items: &[crate::os_shell::MenuItem]) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=gitcraft\n\
         Comment=A small visual git client\n\
         Exec=gitcraft %f\n\
         TryExec=gitcraft\n\
         Terminal=false\n\
         Categories=Development;RevisionControl;\n\
         Icon=gitcraft\n\
         StartupWMClass=gitcraft\n\
         {OURS}\n{}",
        action_sections("gitcraft", items)
    )
}

/// `Actions=…` and a `[Desktop Action …]` section for each of `items`, starting `exec` (the
/// first word of `Exec`, quoted as it must be) with the item's arguments.
fn action_sections(exec: &str, items: &[crate::os_shell::MenuItem]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let ids: Vec<String> = (0..items.len()).map(|i| format!("item{i}")).collect();
    let mut out = format!("Actions={};\n", ids.join(";"));
    for (id, item) in ids.iter().zip(items) {
        let args: Vec<String> = item.args.iter().map(|a| quote_exec(a)).collect();
        out.push_str(&format!(
            "\n[Desktop Action {id}]\nName={}\nExec={exec} {}\n",
            escape_value(&item.title),
            args.join(" ")
        ));
    }
    out
}

fn write_entry(data: &Path, appimage: &Path) -> std::io::Result<()> {
    let icon = icon_path(data);
    if let Some(dir) = icon.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&icon, ICON)?;
    let path = entry_path(data);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    write_whole(&path, &entry(appimage))
}

fn entry(appimage: &Path) -> String {
    let exec = quote_exec(&appimage.to_string_lossy());
    let actions = action_sections(&exec, &actions());
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=gitcraft\n\
         Comment=A small visual git client\n\
         Exec={exec} %f\n\
         TryExec={}\n\
         Terminal=false\n\
         Categories=Development;RevisionControl;\n\
         Icon=gitcraft\n\
         StartupWMClass=gitcraft\n{actions}",
        escape_value(&appimage.to_string_lossy())
    )
}

/// A program path as the quoted first word of `Exec` (Desktop Entry spec: `"`, `` ` ``, `$` and
/// `\` escaped inside the quotes, then the value's own escaping, `%` doubled).
fn quote_exec(path: &str) -> String {
    let mut quoted = String::from("\"");
    for c in path.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    escape_value(&quoted).replace('%', "%%")
}

/// A string value of a desktop file: backslashes and line breaks escaped.
fn escape_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_the_appimage_path() {
        assert_eq!(
            quote_exec("/home/me/Apps/gitcraft.AppImage"),
            "\"/home/me/Apps/gitcraft.AppImage\""
        );
        assert_eq!(
            quote_exec("/home/me/My Apps/$x 100%/a\"b"),
            "\"/home/me/My Apps/\\\\$x 100%%/a\\\\\"b\""
        );
    }

    #[test]
    fn writes_and_refreshes_the_entry() {
        let data = std::env::temp_dir().join(format!("gitcraft-desktop-{}", std::process::id()));
        let old = Path::new("/opt/gitcraft-v0.2.0.AppImage");
        let ok = write_entry(&data, old).is_ok();
        let text = std::fs::read_to_string(entry_path(&data)).unwrap_or_default();
        let icon = std::fs::metadata(icon_path(&data))
            .map(|m| m.len())
            .unwrap_or(0);
        let _ = std::fs::remove_dir_all(&data);
        assert!(ok);
        assert!(text.contains("Exec=\"/opt/gitcraft-v0.2.0.AppImage\" %f\n"));
        assert!(text.contains("TryExec=/opt/gitcraft-v0.2.0.AppImage\n"));
        assert_eq!(icon, ICON.len() as u64);
        assert_ne!(text, entry(Path::new("/opt/gitcraft-v0.2.1.AppImage")));
    }

    #[test]
    fn the_menu_becomes_the_entry_s_actions() {
        let items = crate::os_shell::menu_items(&[
            std::path::PathBuf::from("/home/me/my repo"),
            std::path::PathBuf::from("/home/me/100% $x"),
        ]);
        let text = action_sections("\"/opt/gitcraft.AppImage\"", &items);
        assert_eq!(
            text,
            "Actions=item0;item1;item2;item3;\n\
             \n[Desktop Action item0]\nName=my repo\nExec=\"/opt/gitcraft.AppImage\" \"/home/me/my repo\"\n\
             \n[Desktop Action item1]\nName=100% $x\nExec=\"/opt/gitcraft.AppImage\" \"/home/me/100%% \\\\$x\"\n\
             \n[Desktop Action item2]\nName=Clone…\nExec=\"/opt/gitcraft.AppImage\" \"--clone\"\n\
             \n[Desktop Action item3]\nName=Open…\nExec=\"/opt/gitcraft.AppImage\" \"--open\"\n"
        );
        assert_eq!(action_sections("x", &[]), "");
        let package = package_entry(&items);
        assert!(package.contains(OURS));
        assert!(package.contains("Exec=gitcraft \"--clone\"\n"));
        assert!(package.contains("TryExec=gitcraft\n"));
    }

    #[test]
    fn an_entry_is_written_whole() {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-desktop-whole-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("x.desktop");
        let first = write_whole(&path, "one");
        let second = write_whole(&path, "two");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let left = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(first.is_ok() && second.is_ok());
        assert_eq!(text, "two");
        assert_eq!(left, 1);
    }
}
