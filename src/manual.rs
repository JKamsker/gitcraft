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
        "Practice and glossary",
        &[
            "Practice git… on the start page or in the palette opens a new local throwaway repository: a merge conflict, diverged branches, a detached HEAD or undoing a published commit. The goal banner has a hint and Check goal reads the repository in the background.",
            "The published-commit scenario pushes only to a local bare origin. Your own repositories are untouched. Practice folders stay in the system temporary directory when closed; the system may remove them. Copy a folder elsewhere to keep it.",
            "Git commands and terminals opened from practice tabs ignore inherited Git repository overrides, profiles and global Git configuration. Practice terminals are ordinary shells; commands you choose to run can access other folders.",
            "Pause over words such as HEAD, upstream, fast-forward and rebase anywhere in the window for their meaning. Glossary tips do not appear while typing, scrolling or dragging.",
        ],
    ),
    (
        "Repositories and tabs",
        &[
            "Open, create or clone a repository from the start page; recent ones are listed there.",
            "Clone options choose shallow history (depth), blobless file contents on demand, and sparse folders. The server must support partial clones. Sparse checkout… in the palette edits the folder tree later, with all changes committed or stashed first. Move ignored files outside the working tree too: Git can delete them during sparse changes. Root files and files immediately inside parent folders remain included. Custom non-cone patterns are replaced only when explicitly chosen. Partial clones show when history may download contents; missing files ask before downloading.",
            "The palette exports Git bundles of selected refs or an incremental range (exclude a base commit), and inspects local bundle refs/prerequisites before a deliberate import. Choose new destination refs: existing refs are never overwritten and import does not check out work. File pickers use this machine's disk; SSH/WSL repositories receive temporary copies. Bundle limit: 128 MiB, 1000 header entries and two minutes per Git call. Incremental bundles need their base commits already present.",
            "One tab per repository. Changes made elsewhere (editor, terminal) show up by themselves.",
            "Right-click a tab to give it its own icon and highlight colour.",
            "Drag a tab along the strip to reorder the tabs.",
            "Remote on the start page opens a repository in WSL or over SSH: git runs there, this window shows it. Browse… lists that machine's folders, its repositories marked. The toolbar names the machine; the terminal opens a shell there. What ssh asks (a password, a passphrase, a new host key) is asked in this window; Remember connection keeps a password encrypted in the system's credential store, and Forget saved SSH sign-ins (Ctrl+P) removes them.",
        ],
    ),
    (
        "Gerrit",
        &[
            "For Gerrit, set git config gitcraft.forge gerrit, then reload and fetch. Hosts named gerrit.… and *.googlesource.com are recognised automatically.",
            "Push for review picks the target branch and optional topic, checks every commit above the fetched target for a unique Change-Id, then pushes the exact previewed commit to refs/for/<branch>. Install your server’s commit-msg hook and amend commits with missing IDs. Fetch and rebase divergent or merged series first.",
            "The arrow on a graph row, or Open Gerrit change in its menu, opens the change. Amend last commit keeps its original Change-Id, even with a new description; push for review again to upload another patch set.",
            "For another web address or a reverse-proxy prefix, set git config gitcraft.gerritUrl https://review.example.org/gerrit. Git handles transport and sign-in.",
        ],
    ),
    (
        "Graph",
        &[
            "Every branch with its pill on its latest commit: solid for the checked-out one, a laptop when it is local, a cloud when a remote has it.",
            "Double-click a pill to check that branch out (and pull it when its remote is ahead; on the checked-out branch's remote pill, just pull); hover +N to see every branch of the commit, and right-click any of them.",
            "Right-click a commit or a pill for branches, tags, merge, rebase, cherry-pick, revert, reset and more.",
            "Search messages, authors and hashes, or switch to Code to find the commits that added or removed some text.",
            "Description takes the flexible width; Branch / Tag fits visible pills until you drag its border. Right-click the header to restore Fit visible branch / tag pills, pick columns, make the graph compact or reset widths. Faint lane labels identify offscreen tips; hover a description for the full message.",
            "Interactive rebase: reorder, reword, squash, fix up or drop commits, then start it.",
        ],
    ),
    (
        "History activity",
        &[
            "Activity in the toolbar opens the activity calendar and contributor statistics; both are also in Ctrl+P. Click a day, drag across dates, pick a month or apply a date range. Select a commit to open its detail.",
            "Choose author or committer dates. Days use each timestamp’s recorded timezone, and date ranges include both ends. Authors follow Git’s .mailmap. Counts show distinct paths, active days and first/last activity within the selected period; renames touch both paths and merge changes compare with their first parent.",
            "Each reachable commit counts once across local/remote branches, tags and HEAD. Stash and app-only refs are excluded. Coverage says when the snapshot is partial or shallow. Scan more history expands the initial 5,000 commits up to 100,000, within a 64 MiB snapshot and 16 MiB page limit. Cancel stops waiting; Refresh takes a new snapshot without fetching. Analysis needs Git with --no-lazy-fetch support and reads tree changes without file contents; missing trees or metadata are reported as errors.",
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
            "Profiles (title bar, or Settings → Profile): each has its own tabs, recent repositories, theme, git author, commit signing, SSH key and forge accounts. A repository's own git config (user.name, user.email, signing) still wins over the profile's.",
            "Themes: the palette icon in the title bar, or Settings → Appearance with a thumbnail of each (point at one to preview it). Import a VS Code theme or a Base16 scheme, follow VS Code's theme, or save a copy of one as a file to edit; set your own accent, corners and lane colours for any.",
            "Point at a setting and the panel beside it shows what it does, drawn with the values set now.",
            "Settings → Profile → Profile’s connections chooses a local SSH private key, named gh/glab accounts and a tea login alias. Check account verifies a named forge account in the background. GitHub uses gh’s existing named accounts; glab can use a separate configuration folder for another account on the same server. Selected accounts override inherited token variables per command, without switching a global login. HTTPS Git uses the same accounts. Git in WSL or over SSH keeps that machine’s own credentials.",
            "Settings → Commit signing sets the format, key and program git signs with, and whether it signs commits and tags by default, in your global git config; Settings → Profile can sign just for the profile instead.",
            "Double-click a commit's message in the right pane to edit it (any commit of the checked-out branch with no merges after it).",
            "Undo and redo commits, merges, rebases, resets, checkouts, branches, tags and stashes.",
            "Lost and found by content… in the palette searches literal remembered text across local reflog history, unreachable commits, dropped stashes and unattached staged blobs. Preview provenance and raw content, then deliberately create a new branch/file or return a validated stash to its list. Existing data and the index are preserved; missing/binary/oversized data and search limits are reported, with no automatic downloads.",
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
            "Syntax in a diff compares syntax with Difftastic (difft), installed where the repository is. Changed tokens stand out through reindentation and wrapping. Turn off Syntax to stage or see formatting edits; unsupported languages, parse errors and tool errors show the patch with a note.",
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
            (
                "Cmd+Alt+1 / 2 / 3",
                "Focus sidebar / graph or diff / detail pane",
            ),
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
            ("j / k", "Next / previous file in the focused detail pane"),
            (
                "Space",
                "Stage / unstage the selected file; conflicts are excluded",
            ),
            ("Enter", "Open the selected file's diff"),
            (
                "S / U / D",
                "Stage / unstage / confirm discard of the current diff hunk",
            ),
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
