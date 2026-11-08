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
pub fn file_menu(
    resp: &Response,
    path: &str,
    rev: Option<&str>,
    ignorable: bool,
    diff: Option<&DiffKey>,
    links: &dyn Fn() -> Option<LinkMenu>,
    acts: &mut Vec<Action>,
) {
    file_menu_with(resp, path, rev, ignorable, diff, links, acts, |_, _| {});
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
    acts: &mut Vec<Action>,
    more: impl FnOnce(&mut Ui, &mut Vec<Action>),
) {
    let revert = matches!(diff, Some(DiffKey::Commit { hash, .. }) if rev == Some(hash.as_str()));
    crate::menu_item::context(resp)
        .style(crate::theme::menu_style)
        .show(|ui| {
            let rel = std::path::PathBuf::from(path);
            if crate::menu_item::MenuItem::new("Open file")
                .icon("eye")
                .show(ui)
                .clicked()
            {
                acts.push(Action::OpenPath(rel.clone()));
            }
            if crate::menu_item::MenuItem::new("Open in editor")
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
            if crate::menu_item::MenuItem::new("View file")
                .icon("eye")
                .show(ui)
                .clicked()
            {
                acts.push(Action::FileView {
                    path: path.to_owned(),
                    rev: rev.map(str::to_owned),
                    tab: FileTab::File,
                });
            }
            if crate::menu_item::MenuItem::new("File history")
                .icon("history")
                .show(ui)
                .clicked()
            {
                acts.push(Action::FileView {
                    path: path.to_owned(),
                    rev: rev.map(str::to_owned),
                    tab: FileTab::History,
                });
            }
            if crate::menu_item::MenuItem::new("Blame")
                .icon("history")
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
                let b = crate::menu_item::MenuItem::new("Revert changes to this file")
                    .icon("undo-2")
                    .show(ui)
                    .on_hover_text(
                        "Undo this commit's changes to the file in the working tree, to look over \
                     before committing",
                    );
                if b.clicked() {
                    acts.push(Action::RevertFile {
                        hash: hash.to_owned(),
                        path: path.to_owned(),
                    });
                }
            }
            // Of the working tree: its rules and attributes, not a commit's.
            if rev.is_none()
                && crate::menu_item::MenuItem::new("Ignore rules and attributes")
                    .icon("eye-off")
                    .show(ui)
                    .on_hover_text("Why it is ignored or not, and how git treats it")
                    .clicked()
            {
                acts.push(Action::Doctor(crate::repo::doctor::DoctorAct::Ignore(
                    crate::repo::doctor::IgnoreAct::Explain(path.to_owned()),
                )));
            }
            if ignorable {
                crate::menu_item::MenuItem::new("Ignore")
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
            if crate::menu_item::MenuItem::new("Copy path")
                .icon("copy")
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

/// _Open in diff tool_ or, for a conflicted file, _Resolve in merge tool_.
fn tool_items(ui: &mut egui::Ui, key: &DiffKey, acts: &mut Vec<Action>) {
    match tool_item(key) {
        ToolItem::OpenInDiffTool => {
            if crate::menu_item::MenuItem::new("Open in diff tool")
                .icon("file-diff")
                .show(ui)
                .on_hover_text(
                    "Shows this diff in the diff tool of Settings → Git → External tools",
                )
                .clicked()
            {
                acts.push(Action::OpenInDiffTool(key.clone()));
            }
        }
        ToolItem::ResolveInMergeTool => {
            if crate::menu_item::MenuItem::new("Resolve in merge tool")
                .icon("git-merge")
                .show(ui)
                .on_hover_text(
                    "Opens the conflict in the merge tool of Settings → Git → External tools; \
                     the file is marked resolved when the tool reports success",
                )
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
            "This file".to_owned(),
            format!("/{literal}"),
            path.to_owned(),
        ));
    }
    if let Some(ext) = p.extension().and_then(|e| e.to_str())
        && let Some(literal) = ignore_literal(ext)
    {
        out.push((
            format!("All .{ext} files"),
            format!("*.{literal}"),
            path.to_owned(),
        ));
    }
    let (dir, _) = split_dir(path);
    if !dir.is_empty()
        && let Some(literal) = ignore_literal(dir)
    {
        out.push((
            format!("Folder {dir}/"),
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
