//! _History in gitcraft_ and _Blame in gitcraft_ in Finder's menu of files (Quick Actions,
//! under Services), on macOS: two Automator workflows in `~/Library/Services` that start this
//! app with `log <file>` / `blame -- <file>`. Added and removed from the settings, as a
//! system's own installer would, since a dragged-in app installs nothing.

use std::path::{Path, PathBuf};

/// The two actions: (menu name, the command line's command).
const ACTIONS: [(&str, &str); 2] = [
    ("History in gitcraft", "log"),
    ("Blame in gitcraft", "blame --"),
];

/// Where the user's Quick Actions are kept.
fn services_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|h| h.is_absolute())
        .map(|h| h.join("Library/Services"))
}

fn workflow(services: &Path, name: &str) -> PathBuf {
    services.join(format!("{name}.workflow"))
}

/// Whether the actions can be offered: on macOS, running from an app bundle.
pub fn bundle() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    bundle_of(&exe).map(Path::to_owned)
}

/// The app bundle `exe` is the program of: `X.app/Contents/MacOS/<exe>`.
fn bundle_of(exe: &Path) -> Option<&Path> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let is_bundle = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("app"));
    is_bundle.then_some(bundle)
}

/// Whether both actions are there.
pub fn installed() -> bool {
    services_dir().is_some_and(|s| ACTIONS.iter().all(|(n, _)| workflow(&s, n).is_dir()))
}

/// Adds (or rewrites) the actions.
pub fn install() -> Result<(), String> {
    let services = services_dir().ok_or("No home folder")?;
    write_all(&services)
}

/// Removes the actions.
pub fn uninstall() -> Result<(), String> {
    let services = services_dir().ok_or("No home folder")?;
    for (name, _) in ACTIONS {
        let path = workflow(&services, name);
        match std::fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(())
}

fn write_all(services: &Path) -> Result<(), String> {
    for (name, command) in ACTIONS {
        let contents = workflow(services, name).join("Contents");
        std::fs::create_dir_all(&contents).map_err(|e| format!("{}: {e}", contents.display()))?;
        let files = [
            ("Info.plist", info_plist(name)),
            ("document.wflow", document(&script(command))),
        ];
        for (file, text) in files {
            let path = contents.join(file);
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// The app, as the workflows find it wherever it was moved to: its bundle identifier.
const BUNDLE_ID: &str = "io.github.vancete.gitcraft";

/// The shell script the action runs, the files chosen given as its arguments: a new instance
/// of the app for each (`open -n`; the one running takes them, see `crate::instance`), with
/// the command line asked for.
fn script(command: &str) -> String {
    format!("for f in \"$@\"; do\n\topen -n -b {BUNDLE_ID} --args {command} \"$f\"\ndone\n")
}

/// `text` inside an XML element.
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const PLIST_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n";

/// The workflow's menu entry: shown for files (not folders), in Finder.
fn info_plist(name: &str) -> String {
    format!(
        "{PLIST_HEAD}<dict>\n\
         \t<key>NSServices</key>\n\t<array>\n\t\t<dict>\n\
         \t\t\t<key>NSMenuItem</key>\n\t\t\t<dict>\n\
         \t\t\t\t<key>default</key>\n\t\t\t\t<string>{}</string>\n\t\t\t</dict>\n\
         \t\t\t<key>NSMessage</key>\n\t\t\t<string>runWorkflowAsService</string>\n\
         \t\t\t<key>NSRequiredContext</key>\n\t\t\t<dict>\n\
         \t\t\t\t<key>NSApplicationIdentifier</key>\n\
         \t\t\t\t<string>com.apple.finder</string>\n\t\t\t</dict>\n\
         \t\t\t<key>NSSendFileTypes</key>\n\t\t\t<array>\n\
         \t\t\t\t<string>public.data</string>\n\t\t\t</array>\n\
         \t\t</dict>\n\t</array>\n</dict>\n</plist>\n",
        xml(name)
    )
}

/// The workflow itself: one Run Shell Script action, the files passed as arguments.
fn document(script: &str) -> String {
    format!(
        "{PLIST_HEAD}<dict>\n\
         \t<key>AMApplicationBuild</key>\n\t<string>523</string>\n\
         \t<key>AMApplicationVersion</key>\n\t<string>2.10</string>\n\
         \t<key>AMDocumentVersion</key>\n\t<string>2</string>\n\
         \t<key>actions</key>\n\t<array>\n\t\t<dict>\n\
         \t\t\t<key>action</key>\n\t\t\t<dict>\n\
         \t\t\t\t<key>AMAccepts</key>\n\t\t\t\t<dict>\n\
         \t\t\t\t\t<key>Container</key>\n\t\t\t\t\t<string>List</string>\n\
         \t\t\t\t\t<key>Optional</key>\n\t\t\t\t\t<true/>\n\
         \t\t\t\t\t<key>Types</key>\n\t\t\t\t\t<array>\n\
         \t\t\t\t\t\t<string>com.apple.cocoa.path</string>\n\t\t\t\t\t</array>\n\
         \t\t\t\t</dict>\n\
         \t\t\t\t<key>AMActionVersion</key>\n\t\t\t\t<string>2.0.3</string>\n\
         \t\t\t\t<key>AMProvides</key>\n\t\t\t\t<dict>\n\
         \t\t\t\t\t<key>Container</key>\n\t\t\t\t\t<string>List</string>\n\
         \t\t\t\t\t<key>Types</key>\n\t\t\t\t\t<array>\n\
         \t\t\t\t\t\t<string>com.apple.cocoa.string</string>\n\t\t\t\t\t</array>\n\
         \t\t\t\t</dict>\n\
         \t\t\t\t<key>ActionBundlePath</key>\n\
         \t\t\t\t<string>/System/Library/Automator/Run Shell Script.action</string>\n\
         \t\t\t\t<key>ActionName</key>\n\t\t\t\t<string>Run Shell Script</string>\n\
         \t\t\t\t<key>ActionParameters</key>\n\t\t\t\t<dict>\n\
         \t\t\t\t\t<key>COMMAND_STRING</key>\n\t\t\t\t\t<string>{}</string>\n\
         \t\t\t\t\t<key>CheckedForUserDefaultShell</key>\n\t\t\t\t\t<true/>\n\
         \t\t\t\t\t<key>inputMethod</key>\n\t\t\t\t\t<integer>1</integer>\n\
         \t\t\t\t\t<key>shell</key>\n\t\t\t\t\t<string>/bin/sh</string>\n\
         \t\t\t\t\t<key>source</key>\n\t\t\t\t\t<string></string>\n\
         \t\t\t\t</dict>\n\
         \t\t\t\t<key>BundleIdentifier</key>\n\
         \t\t\t\t<string>com.apple.RunShellScript</string>\n\
         \t\t\t\t<key>CFBundleVersion</key>\n\t\t\t\t<string>2.0.3</string>\n\
         \t\t\t\t<key>CanShowSelectedItemsWhenRun</key>\n\t\t\t\t<false/>\n\
         \t\t\t\t<key>CanShowWhenRun</key>\n\t\t\t\t<true/>\n\
         \t\t\t\t<key>Class Name</key>\n\t\t\t\t<string>RunShellScriptAction</string>\n\
         \t\t\t\t<key>InputUUID</key>\n\
         \t\t\t\t<string>6B1F0C1E-6A1D-4C1E-9C1E-0A1B2C3D4E5F</string>\n\
         \t\t\t\t<key>OutputUUID</key>\n\
         \t\t\t\t<string>7C2A1D2F-7B2E-4D2F-8D2F-1B2C3D4E5F60</string>\n\
         \t\t\t\t<key>UUID</key>\n\
         \t\t\t\t<string>8D3B2E30-8C3F-4E30-9E30-2C3D4E5F6071</string>\n\
         \t\t\t</dict>\n\
         \t\t\t<key>isViewVisible</key>\n\t\t\t<false/>\n\
         \t\t</dict>\n\t</array>\n\
         \t<key>connectors</key>\n\t<dict/>\n\
         \t<key>workflowMetaData</key>\n\t<dict>\n\
         \t\t<key>serviceApplicationBundleID</key>\n\t\t<string>com.apple.finder</string>\n\
         \t\t<key>serviceInputTypeIdentifier</key>\n\
         \t\t<string>com.apple.Automator.fileSystemObject</string>\n\
         \t\t<key>serviceOutputTypeIdentifier</key>\n\
         \t\t<string>com.apple.Automator.nothing</string>\n\
         \t\t<key>serviceProcessesInput</key>\n\t\t<integer>0</integer>\n\
         \t\t<key>workflowTypeIdentifier</key>\n\
         \t\t<string>com.apple.Automator.servicesMenu</string>\n\
         \t</dict>\n</dict>\n</plist>\n",
        xml(script)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_a_program_is_in() {
        assert_eq!(
            bundle_of(Path::new("/A/gitcraft.app/Contents/MacOS/gitcraft")),
            Some(Path::new("/A/gitcraft.app"))
        );
        assert_eq!(bundle_of(Path::new("/A/x.app/gitcraft")), None);
        assert_eq!(bundle_of(Path::new("/usr/bin/gitcraft")), None);
    }

    #[test]
    fn the_script_starts_the_app_for_each_file() {
        assert_eq!(
            script("blame --"),
            "for f in \"$@\"; do\n\topen -n -b io.github.vancete.gitcraft --args blame -- \
             \"$f\"\ndone\n"
        );
    }

    #[test]
    fn writes_and_removes_both_workflows() -> Result<(), String> {
        let services =
            std::env::temp_dir().join(format!("gitcraft-services &' {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&services);
        let written = write_all(&services);
        let read = |name: &str, file: &str| {
            std::fs::read_to_string(workflow(&services, name).join("Contents").join(file))
                .unwrap_or_default()
        };
        let info = read("History in gitcraft", "Info.plist");
        let doc = read("Blame in gitcraft", "document.wflow");
        let _ = std::fs::remove_dir_all(&services);
        written?;
        assert!(info.contains("<string>History in gitcraft</string>"));
        assert!(info.contains("<string>runWorkflowAsService</string>"));
        assert!(info.contains("<string>public.data</string>"));
        // The script, escaped for XML.
        assert!(doc.contains("--args blame -- &quot;$f&quot;"), "{doc}");
        assert!(doc.contains("&quot;$@&quot;"), "{doc}");
        assert_eq!(xml("a&b<\"c\">"), "a&amp;b&lt;&quot;c&quot;&gt;");
        Ok(())
    }
}
