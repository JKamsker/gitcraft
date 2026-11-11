use super::*;

impl RepoTab {
    /// Side by side and the whole file are kept for next time; whitespace is per tab.
    pub(in crate::repo) fn set_diff_options(
        &mut self,
        split: bool,
        ignore_ws: bool,
        whole_file: bool,
        ctx: &egui::Context,
    ) {
        let mut settings = Settings::get(ctx);
        let reload = ignore_ws != self.diff_ignore_ws || whole_file != settings.diff_whole_file;
        settings.diff_split = split;
        settings.diff_whole_file = whole_file;
        settings.set(ctx);
        self.diff_ignore_ws = ignore_ws;
        if reload {
            self.reopen_diff(ctx);
        }
    }

    /// Adds `pattern` to `.gitignore`, and stops tracking `path` if git tracks it.
    pub(in crate::repo) fn ignore(&mut self, pattern: &str, path: &str, ctx: &egui::Context) {
        // On another machine, the file is read and written there, in the background.
        let (repo, pattern, path) = (self.path.clone(), pattern.to_owned(), path.to_owned());
        self.here_or_later(
            ctx,
            move || {
                let added = git::add_to_gitignore(&repo, &pattern);
                let tracked = added.is_ok() && git::is_tracked(&repo, &path);
                (pattern, path, added, tracked)
            },
            |tab, (pattern, path, added, tracked), ctx| {
                tab.ignored(&pattern, &path, added, tracked, ctx);
            },
        );
    }

    /// Stops tracking `path` once `pattern` is in `.gitignore`, if it was tracked.
    pub(in crate::repo) fn ignored(
        &mut self,
        pattern: &str,
        path: &str,
        added: Result<bool, String>,
        tracked: bool,
        ctx: &egui::Context,
    ) {
        match added {
            Err(e) => self.status = Some(crate::repo::Notice::plain((e, true))),
            Ok(added) => {
                if tracked {
                    self.run_prepared(
                        &format!("Ignore {pattern}"),
                        &crate::i18n::message(
                            "repo-core-0156",
                            &[
                                ("pattern", pattern.to_string().into()),
                                ("path", path.to_string().into()),
                            ],
                        ),
                        ops(&[&[
                            "--literal-pathspecs",
                            "rm",
                            "-r",
                            "--cached",
                            "--quiet",
                            "--",
                            path,
                        ]]),
                        Refresh::Status,
                        OpKind::plain(),
                        ctx,
                    );
                } else {
                    let msg = if added {
                        crate::i18n::message(
                            "repo-core-0162",
                            &[("pattern", pattern.to_string().into())],
                        )
                    } else {
                        crate::i18n::message(
                            "repo-core-0163",
                            &[("pattern", pattern.to_string().into())],
                        )
                    };
                    self.status = Some(crate::repo::Notice::plain((msg, false)));
                    self.reload_status(ctx);
                }
            }
        }
    }
}

/// Runs the user's editor command with `path` (through `cmd` on Windows, where editors are
/// usually `.cmd` scripts).
pub(crate) fn open_in_editor(editor: &str, path: &Path) -> Result<(), String> {
    crate::remote::openable_here(path)?;
    let editor = if editor.trim().is_empty() {
        "code"
    } else {
        editor.trim()
    };
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("cmd");
        // The path in quotes and through a variable: a file named `a&calc` must not run `calc`,
        // and one named `%COMSPEC%` must not open another (cmd expands `%…%` even in quotes,
        // but not again inside what a variable held). `/S` takes off only the outer quotes,
        // leaving `editor "path"`.
        let mut target = path.as_os_str().to_owned();
        // A backslash before the closing quote would escape it for the editor (the folder
        // itself ends with one): doubled, it is just a backslash.
        if target.to_string_lossy().ends_with('\\') {
            target.push("\\");
        }
        c.env("GITCRAFT_OPEN_PATH", target);
        let mut line = std::ffi::OsString::from("/S /C \"");
        line.push(editor);
        line.push(" \"%GITCRAFT_OPEN_PATH%\"\"");
        c.raw_arg(line);
        c.creation_flags(0x0800_0000);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        // A command with arguments (`code -n`, `flatpak run …`), as `cmd` reads it on Windows:
        // through the shell, the path still one argument (`$1`) it does not read. A program
        // whose own path has a space in it is run as it is.
        let mut c = if editor.contains(char::is_whitespace) && !Path::new(editor).exists() {
            let mut c = std::process::Command::new("sh");
            c.arg("-c").arg(format!("{editor} \"$1\"")).arg("sh");
            c
        } else {
            std::process::Command::new(editor)
        };
        c.arg(path);
        crate::shell_path::apply(&mut c);
        c
    };
    let mut child = cmd.spawn().map_err(|e| {
        crate::i18n::message(
            "repo-core-0191",
            &[
                ("editor", editor.to_string().into()),
                ("e", format!("{}", e).into()),
            ],
        )
    })?;
    // Reaped once it ends (most editor commands hand the file over and exit at once), or each
    // file opened would leave a zombie process for as long as the app runs.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
