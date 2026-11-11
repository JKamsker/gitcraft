//! The right-click menu of a file, and the `.gitignore` lines it offers.

use egui::{Response, Ui};

use super::file_row::split_dir;
use crate::forge::links::Target;
use crate::repo::external_tools::{ToolItem, tool_item};
use crate::repo::file_view::FileTab;
use crate::repo::forge_link::LinkMenu;
use crate::repo::{Action, DiffKey};

/// Right-click menu for a file of the working tree or a commit: open it, its folder, copy path.
/// Right-click menu of a file. `rev`: the commit it is shown in (it is viewed as it is there,
/// history and blame start there);
/// `ignorable`: whether the file can be ignored; `diff`: the external tool target.
/// `links`: forge destinations, read when the menu opens.
#[allow(clippy::too_many_arguments)]
pub fn file_menu(
    resp: &Response,
    path: &str,
    rev: Option<&str>,
    ignorable: bool,
    diff: Option<&DiffKey>,
    links: &dyn Fn() -> Option<LinkMenu>,
    shortcuts: bool,
    acts: &mut Vec<Action>,
) {
    file_menu_with(
        resp,
        path,
        rev,
        ignorable,
        diff,
        links,
        shortcuts,
        acts,
        |_, _| {},
    );
}

/// File menu with additional actions such as LFS locks and stashing.
#[allow(clippy::too_many_arguments)]
pub fn file_menu_with(
    resp: &Response,
    path: &str,
    rev: Option<&str>,
    ignorable: bool,
    diff: Option<&DiffKey>,
    links: &dyn Fn() -> Option<LinkMenu>,
    shortcuts: bool,
    acts: &mut Vec<Action>,
    more: impl FnOnce(&mut Ui, &mut Vec<Action>),
) {
    if shortcuts && resp.has_focus() && !crate::a11y::keyboard_taken(&resp.ctx) {
        selection_commands(&resp.ctx, path, rev, acts);
    }
    let revert = matches!(diff, Some(DiffKey::Commit { hash, .. }) if rev == Some(hash.as_str()));
    crate::menu_item::context(resp)
        .style(crate::theme::menu_style)
        .show(|ui| {
            let rel = std::path::PathBuf::from(path);
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3749"))
                .icon("eye")
                .show(ui)
                .clicked()
            {
                acts.push(Action::OpenPath(rel.clone()));
            }
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-0113"))
                .icon("code")
                .show(ui)
                .clicked()
            {
                acts.push(Action::OpenInEditor(rel.clone()));
            }
            if let Some(key) = diff {
                tool_items(ui, key, acts);
            }
            if crate::menu_item::MenuItem::new(crate::repo::reveal_label())
                .icon("folder-open")
                .show(ui)
                .clicked()
            {
                acts.push(Action::OpenPath(
                    rel.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
                ));
            }
            ui.separator();
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3750"))
                .icon("eye")
                .keys_when(crate::keymap::Command::FileContents, shortcuts)
                .show(ui)
                .clicked()
            {
                acts.push(Action::FileView {
                    path: path.to_owned(),
                    rev: rev.map(str::to_owned),
                    tab: FileTab::File,
                });
            }
            if let Some(rev) = rev
                && ui.button(crate::i18n::text("repo-core-3751")).clicked()
            {
                acts.push(Action::RepositoryTools(
                    crate::repo::repository_tools::Act::Object(format!("{rev}:{path}")),
                ));
            }
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3752"))
                .icon("history")
                .keys_when(crate::keymap::Command::FileHistory, shortcuts)
                .show(ui)
                .clicked()
            {
                acts.push(Action::FileView {
                    path: path.to_owned(),
                    rev: rev.map(str::to_owned),
                    tab: FileTab::History,
                });
            }
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2498"))
                .icon("history")
                .keys_when(crate::keymap::Command::FileBlame, shortcuts)
                .show(ui)
                .clicked()
            {
                acts.push(Action::FileView {
                    path: path.to_owned(),
                    rev: rev.map(str::to_owned),
                    tab: FileTab::Blame,
                });
            }
            if revert && let Some(hash) = rev {
                ui.separator();
                let b = crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3753"))
                    .icon("undo-2")
                    .show(ui)
                    .on_hover_text(crate::i18n::text("repo-core-2381"));
                if b.clicked() {
                    acts.push(Action::RevertFile {
                        hash: hash.to_owned(),
                        path: path.to_owned(),
                    });
                }
            }
            // Of the working tree: its rules and attributes, not a commit's.
            if rev.is_none()
                && crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3754"))
                    .icon("eye-off")
                    .show(ui)
                    .on_hover_text(crate::i18n::text("repo-core-3755"))
                    .clicked()
            {
                acts.push(Action::Doctor(crate::repo::doctor::DoctorAct::Ignore(
                    crate::repo::doctor::IgnoreAct::Explain(path.to_owned()),
                )));
            }
            if ignorable {
                crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3756"))
                    .icon("eye-off")
                    .submenu(ui, |ui| {
                        for (label, pattern, untrack) in ignore_patterns(path) {
                            if crate::menu_item::MenuItem::new(label)
                                .icon("command")
                                .show(ui)
                                .clicked()
                            {
                                acts.push(Action::Ignore {
                                    pattern,
                                    path: untrack,
                                });
                            }
                        }
                    });
            }
            ui.separator();
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3525"))
                .icon("copy")
                .keys_when(crate::keymap::Command::FileCopyPath, shortcuts)
                .show(ui)
                .clicked()
            {
                ui.ctx().copy_text(path.to_owned());
            }
            let scope = crate::custom_commands::Scope {
                sha: rev.unwrap_or_default().to_owned(),
                file: path.to_owned(),
                ..Default::default()
            };
            crate::repo::custom_commands::menu_items(
                ui,
                crate::custom_commands::Placement::File,
                scope,
                acts,
            );
            if let Some(menu) = links() {
                let target = Target::File {
                    rev: rev.unwrap_or("HEAD").to_owned(),
                    path: path.to_owned(),
                    lines: None,
                };
                menu.items(ui, "file", &target, acts);
            }
            more(ui, acts);
        });
}

/// Read-only actions capture the selected path/revision and use the same normal actions as
/// the context menu. No file/Git/process work is performed while reading keyboard input.
pub(in crate::repo) fn selection_commands(
    ctx: &egui::Context,
    path: &str,
    rev: Option<&str>,
    acts: &mut Vec<Action>,
) {
    use crate::keymap::{self, Command};
    for (command, tab) in [
        (Command::FileContents, FileTab::File),
        (Command::FileHistory, FileTab::History),
        (Command::FileBlame, FileTab::Blame),
    ] {
        if keymap::take(ctx, command) {
            acts.push(Action::FileView {
                path: path.to_owned(),
                rev: rev.map(str::to_owned),
                tab,
            });
        }
    }
    if keymap::take(ctx, Command::FileCopyPath) {
        ctx.copy_text(path.to_owned());
    }
}

/// _Open in diff tool_ or, for a conflicted file, _Resolve in merge tool_.
fn tool_items(ui: &mut egui::Ui, key: &DiffKey, acts: &mut Vec<Action>) {
    match tool_item(key) {
        ToolItem::OpenInDiffTool => {
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3757"))
                .icon("file-diff")
                .show(ui)
                .on_hover_text(crate::i18n::text("repo-core-3758"))
                .clicked()
            {
                acts.push(Action::OpenInDiffTool(key.clone()));
            }
        }
        ToolItem::ResolveInMergeTool => {
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-0730"))
                .icon("git-merge")
                .show(ui)
                .on_hover_text(crate::i18n::text("repo-core-3759"))
                .clicked()
            {
                acts.push(Action::ResolveInMergeTool(key.path().to_owned()));
            }
        }
        ToolItem::None => {}
    }
}

/// The ways to ignore `path`: (menu label, `.gitignore` line, what to stop tracking).
fn ignore_patterns(path: &str) -> Vec<(String, String, String)> {
    let p = std::path::Path::new(path);
    let mut out = Vec::new();
    if let Some(literal) = ignore_literal(path) {
        out.push((
            crate::i18n::text("repo-core-3760").to_owned(),
            format!("/{literal}"),
            path.to_owned(),
        ));
    }
    if let Some(ext) = p.extension().and_then(|e| e.to_str())
        && let Some(literal) = ignore_literal(ext)
    {
        out.push((
            crate::i18n::message("repo-core-3762", &[("ext", ext.to_string().into())]),
            format!("*.{literal}"),
            path.to_owned(),
        ));
    }
    let (dir, _) = split_dir(path);
    if !dir.is_empty()
        && let Some(literal) = ignore_literal(dir)
    {
        out.push((
            crate::i18n::message("repo-core-3764", &[("dir", dir.to_string().into())]),
            format!("/{literal}/"),
            dir.to_owned(),
        ));
    }
    out
}

/// `name` as a `.gitignore` pattern matching just it: `*`, `?`, `[` and `\` stand for
/// themselves, and so do spaces at the end (which git would drop). `None` for a name with a
/// line break, which a `.gitignore` line can't hold.
fn ignore_literal(name: &str) -> Option<String> {
    if name.contains(['\n', '\r']) {
        return None;
    }
    let body = name.trim_end_matches(' ');
    let mut out = String::with_capacity(name.len() + 4);
    for c in body.chars() {
        if matches!(c, '*' | '?' | '[' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    for _ in body.len()..name.len() {
        out.push_str("\\ ");
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(path: &str) -> Vec<String> {
        ignore_patterns(path)
            .into_iter()
            .map(|(_, p, _)| p)
            .collect()
    }

    #[test]
    fn plain_paths_ignore_as_before() {
        assert_eq!(patterns("src/a.log"), ["/src/a.log", "*.log", "/src/"]);
        assert_eq!(patterns("README"), ["/README"]);
    }

    #[test]
    fn glob_characters_in_a_name_match_only_themselves() {
        // Unescaped, `/[ab].txt` would ignore `a.txt` and `b.txt` but not the file itself.
        assert_eq!(patterns("[ab].txt"), ["/\\[ab].txt", "*.txt"]);
        assert_eq!(patterns("x*?/y"), ["/x\\*\\?/y", "/x\\*\\?/"]);
    }

    #[test]
    fn trailing_spaces_are_kept() {
        assert_eq!(patterns("notes  "), ["/notes\\ \\ "]);
    }

    #[test]
    fn a_name_with_a_line_break_is_never_written_to_gitignore() {
        // `a\n*` would add a second line ignoring everything.
        assert!(patterns("a\n*").is_empty());
        assert_eq!(patterns("d\nx/f.rs"), ["*.rs"]);
    }

    #[test]
    fn a_nested_repository_is_ignored_as_a_folder() {
        assert_eq!(patterns("lib/sub/"), ["/lib/sub/", "/lib/"]);
    }
}

#[cfg(test)]
mod registry_menu_tests {
    use super::*;
    use crate::keymap::{self, Command, Scope, Stroke};
    use egui::{Key, Modifiers};
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_file_menu_shows_only_targeted_shortcuts_and_captures_history() {
        let mut h = Harness::new_ui_state(
            |ui, state: &mut (bool, bool, Vec<Action>)| {
                if !std::mem::replace(&mut state.0, true) {
                    egui_extras::install_image_loaders(ui.ctx());
                    let mut settings = crate::settings::Settings::default();
                    settings.key_bindings.insert(
                        Command::FileHistory.id().into(),
                        vec![Stroke::new(Key::F8, Modifiers::NONE)],
                    );
                    settings.put_for_test(ui.ctx());
                }
                keymap::active(
                    ui.ctx(),
                    vec![Scope::Window, Scope::Repository, Scope::Files],
                );
                let response = ui.button("File row");
                file_menu(
                    &response,
                    "recorded.txt",
                    Some("recorded-revision"),
                    false,
                    None,
                    &|| None,
                    state.1,
                    &mut state.2,
                );
            },
            (false, true, Vec::new()),
        );
        h.run();
        h.key_press(Key::Tab);
        h.run();
        h.key_press_modifiers(Modifiers::SHIFT, Key::F10);
        h.run();
        h.get_by_label("File history F8");
        h.state_mut().1 = false;
        h.run();
        h.get_by_label("File history");
        assert!(
            h.query_by_label("File history F8").is_none(),
            "another row's shortcut must not be promised"
        );
        h.get_by_label("File history").click();
        h.run();
        assert!(
            matches!(h.state().2.as_slice(), [Action::FileView { path, rev, tab: FileTab::History }] if path == "recorded.txt" && rev.as_deref() == Some("recorded-revision"))
        );
    }
}
