//! The manual (F1, or the book in the title bar): what gitcraft does and its keyboard shortcuts.

use egui::{RichText, Ui};

use crate::theme::{self, Tokens};

/// Ctrl, or Cmd on macOS, as the shortcuts are written.
pub(crate) fn command() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

/// What each part of the window does: (title, points).
const FEATURES: &[(&str, &[&str])] = &[
    (
        "Repositories and tabs",
        &[
            "Open, create or clone a repository from the start page; recent ones are listed there.",
            "One tab per repository. Changes made elsewhere (editor, terminal) show up by themselves.",
            "Right-click a tab to give it its own icon and highlight colour.",
            "Drag a tab along the strip to reorder the tabs.",
            "Remote on the start page opens a repository in WSL or over SSH: git runs there, this window shows it. Browse… lists that machine's folders, its repositories marked. The toolbar names the machine; the terminal opens a shell there. What ssh asks (a password, a passphrase, a new host key) is asked in this window; Remember connection keeps a password encrypted in the system's credential store, and Forget saved SSH sign-ins (Ctrl+P) removes them.",
        ],
    ),
    (
        "Graph",
        &[
            "Every branch with its pill on its latest commit: solid for the checked-out one, a laptop when it is local, a cloud when a remote has it.",
            "Double-click a pill to check that branch out (and pull it when its remote is ahead; on the checked-out branch's remote pill, just pull); hover +N to see every branch of the commit, and right-click any of them.",
            "Right-click a commit or a pill for branches, tags, merge, rebase, cherry-pick, revert, reset and more.",
            "Search messages, authors and hashes, or switch to Code to find the commits that added or removed some text.",
            "Drag the borders in the column header to resize; right-click it to pick the columns, make the graph column compact or reset the columns.",
            "Interactive rebase: reorder, reword, squash, fix up or drop commits, then start it.",
        ],
    ),
    (
        "Compare",
        &[
            "Ctrl+click a second commit to list the files that differ between the two (the changes row is the working tree).",
            "On a branch's menu, “Changes x adds to main” shows what it changed since they diverged, like a pull request; “Compare main with x” shows every difference.",
        ],
    ),
    (
        "Changes and commits",
        &[
            "Stage, unstage or discard whole files from the right pane.",
            "Path / Tree at the top of the right pane lists the files flat or in their folders; click a folder to fold it.",
            "In a file's diff, stage, unstage or discard one hunk, or click the lines you want and act on only those.",
            "A file kept in Git LFS shows as its LFS objects and their sizes; the palette has LFS: Fetch and LFS: Pull when git-lfs is installed.",
            "The arrows in a diff's header jump from change to change; the marks beside the scrollbar show where the changes are, and a click on one jumps there.",
            "Write the summary and description and commit; amend the last commit; pick the identity you commit as, or Use the profile's author to take a repository's own one out.",
            "Profiles (title bar, or Settings → Profile): each has its own tabs, recent repositories, theme, git author and commit signing. A repository's own git config (user.name, user.email, signing) still wins over the profile's.",
            "Themes: the palette icon in the title bar, or Settings → Appearance with a thumbnail of each (point at one to preview it). Import a VS Code theme or a Base16 scheme, follow VS Code's theme, or save a copy of one as a file to edit; set your own accent, corners and lane colours for any.",
            "Point at a setting and the panel beside it shows what it does, drawn with the values set now.",
            "Settings → Commit signing sets the format, key and program git signs with, and whether it signs commits and tags by default, in your global git config; Settings → Profile can sign just for the profile instead.",
            "Double-click a commit's message in the right pane to edit it (any commit of the checked-out branch with no merges after it).",
            "Fold staged changes into an older commit: right-click it in the graph and pick “Fold staged changes into this commit”, or use “Fold…” beside “Amend last commit”. First it shows the commit and the ones after it that get rewritten, and warns when they are already pushed. “Keep as fixup! commit for later” only commits them as “fixup! <summary>” on top. A conflict is resolved as in any rebase, and Abort goes back to before the fold; Ctrl+Z undoes it, the changes staged again.",
            "Undo and redo commits, merges, rebases, folds, resets, checkouts, branches, tags and stashes.",
            "Reflog (left pane, a branch's menu, or Ctrl+P): where HEAD and each branch have been, what moved them (commit, checkout, reset, rebase…) and from which commit to which, also for what was done in the terminal or before gitcraft was opened. Select an entry to see its commit; Create branch here (or a double-click) gets it back, or cherry-pick it, check it out or copy its hash.",
            "Find lost commits scans for the commits no branch, tag or stash has any more, even once their reflog has expired, newest first, with the same buttons; a dropped stash is applied back with Apply stash. It reads every object, so on a big repository it takes a while; Cancel stops it.",
            "Before a reset, deleting a branch (here or on a remote), dropping a commit or discarding a file, the dialog says what it will do: the commits leaving the branch (for a delete, those the checked-out branch hasn't), each marked still on another branch or only here; the files whose uncommitted changes are lost; whether the commits it rewrites were pushed, so that a force push will be needed; the git commands it runs (folded); and whether Ctrl+Z restores it.",
        ],
    ),
    (
        "Branches, remotes and more",
        &[
            "Left pane: local branches with ahead / behind counts, remotes, worktrees, submodules, stashes, pull requests and tags.",
            "Submodules show whether they are modified, not initialised or out of date; right-click one to update it or open it in a tab. Settings → Git can keep them up to date after each checkout, pull, merge and rebase.",
            "Hover a branch and click its eye to hide it from the graph, or Solo it from its menu to see only the branches soloed; “Show all” at the top brings them all back.",
            "Conflicts open in a three-pane editor with Ours, Theirs and Both for each conflict.",
            "Right-click a file for its history, its blame or the whole file as it is at the commit.",
            "Switch a diff from Diff to File to read the whole file; tick View all files under a commit's files to list every file in it.",
            "In a Markdown file's diff, the eye button renders it: its changes block by block, or with File the whole file.",
            "Sync (toolbar, or the command palette) pulls the current branch the way Pull is set to, then pushes it; if the pull fails (conflicts, or it can't fast-forward), nothing is pushed.",
            "A push the remote refuses because it has commits you don't have opens a banner: how many, and whether your branch was rewritten here (amended, rebased, undone) or both moved on, or is only behind. Pull, then push joins them; Force push (with lease)… names the commits it removes from the remote first, and pushes nothing if the remote (or your branch) moved since it was checked.",
            "A push a hook or a protected branch refuses shows git's message in full, with no force push offered. A branch on no remote yet, with several remotes, asks which one to publish it on.",
            "Right-click a pull request and choose Open in worktree to work on its branch in a worktree of its own, made for it when there is none.",
            "A terminal panel with tabs, in the repository's folder.",
        ],
    ),
];

/// Keyboard and mouse shortcuts: (keys, what they do). `Cmd` stands for Ctrl outside macOS.
pub(crate) const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Window",
        &[
            ("Cmd+T", "New tab"),
            ("Cmd+W", "Close the tab (or middle-click it)"),
            ("Cmd+Shift+T", "Reopen the tab closed last"),
            ("Cmd+P", "Command palette: every action and branch"),
            ("Cmd+,", "Settings"),
            ("F5", "Reload the repository"),
            ("F1", "This manual"),
        ],
    ),
    (
        "Graph",
        &[
            ("↑ / ↓", "Previous / next commit"),
            ("Cmd+F", "Search commits"),
            ("Enter / Shift+Enter", "Next / previous search result"),
            ("Cmd+click", "Compare with the selected commit"),
            ("Double-click a pill", "Check out that branch or tag"),
            ("Esc", "Close the diff, comparison or search"),
        ],
    ),
    (
        "Changes",
        &[
            ("Cmd+Enter", "Commit (from the message fields)"),
            (
                "Click / Shift+click",
                "Pick lines in a diff, or a range of them",
            ),
            ("F7 / Shift+F7", "Next / previous change in a diff"),
            ("Alt+↓ / Alt+↑", "Next / previous change in a diff"),
            ("Cmd+Z", "Undo"),
            ("Cmd+Y / Cmd+Shift+Z", "Redo"),
        ],
    ),
    (
        "Terminal",
        &[
            (
                "Cmd+C",
                "Copy the selection (without one, it goes to the shell)",
            ),
            ("Cmd+Shift+C", "Copy the selection"),
        ],
    ),
    (
        "Settings",
        &[
            ("Cmd+F", "Search the settings"),
            ("Esc", "Clear the search"),
            ("Alt+↑ / Alt+↓", "Previous / next page"),
        ],
    ),
];

/// Draws the manual's contents.
pub fn ui(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let heading = |ui: &mut Ui, text: &str| {
        ui.add_space(10.0);
        ui.label(
            RichText::new(text.to_uppercase())
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        ui.add_space(2.0);
    };

    for (title, points) in FEATURES {
        heading(ui, title);
        for point in *points {
            // The bullet on the left, the text wrapping beside it.
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new("•").color(t.text_faint));
                ui.add(
                    egui::Label::new(
                        RichText::new(point.replace("Ctrl", command())).color(t.text_dim),
                    )
                    .wrap(),
                );
            });
        }
    }

    ui.add_space(8.0);
    ui.separator();
    for (title, keys) in SHORTCUTS {
        heading(ui, &format!("{title} shortcuts"));
        egui::Grid::new(("manual_keys", *title))
            .num_columns(2)
            .spacing([16.0, 6.0])
            .min_col_width(170.0)
            .show(ui, |ui| {
                for (combo, what) in *keys {
                    ui.horizontal(|ui| key_combo(ui, &combo.replace("Cmd", command())));
                    ui.label(RichText::new(*what).color(t.text_dim));
                    ui.end_row();
                }
            });
    }
}

/// Key combinations, `a / b`, each drawn like a keycap.
pub(crate) fn key_combo(ui: &mut Ui, combo: &str) {
    let t = Tokens::get(ui.ctx());
    ui.spacing_mut().item_spacing.x = 4.0;
    for (n, part) in combo.split(" / ").enumerate() {
        if n > 0 {
            ui.label(RichText::new("/").color(t.text_faint));
        }
        key_chip(ui, part);
    }
}

/// A key combination drawn like a keycap.
fn key_chip(ui: &mut Ui, keys: &str) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.field)
        .stroke(egui::Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(keys).font(theme::mono(11.5)).color(t.text));
        });
}
