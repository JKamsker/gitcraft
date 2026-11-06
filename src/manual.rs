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
        "Bisect",
        &[
            "Right-click a commit and choose Start bisect here… to find the commit that brought a change in: say whether it is good (from before the change) or bad (it has the change), and give the other end (HEAD is the bad one unless you name another). It refuses while there are uncommitted changes, conflicts, or a merge or rebase in progress.",
            "A banner over the graph then names the commit checked out for you to test and about how many steps are left; Good, Bad and Skip mark it, and right-clicking another commit marks that one. The graph tints good commits green, bad ones red and skipped ones grey.",
            "Run a command… in the banner lets git bisect run test each commit for you, on the repository's machine (here, in WSL or over SSH): exit code 0 is good, 125 skips the commit, any other code up to 127 is bad, and 128 or more stops. Its output shows under the banner as it comes; Stop run stops it, and the bisect stays where it got to.",
            "At the end the first bad commit is selected and its details open. Finish (or Stop, which asks first, before the end) runs git bisect reset, which checks out again what was checked out when it began.",
            "A bisect started in a terminal shows in the banner as soon as gitcraft sees it, and goes on from there; each worktree has its own.",
        ],
    ),
    (
        "Tests over history",
        &[
            "Right-click a commit and choose Test commits up to here… (or Test commits… in the palette) to run a command, such as cargo test, at each commit from the base branch to it. It runs on the repository's machine (here, in WSL or over SSH), in scratch worktrees in that machine's temporary folder, several commits at once (at most 4 on another machine), in the background: you can keep working. Exit code 0 passes; any other fails.",
            "The graph then has a test lane: a small square by each commit, green with a tick when it passed, red with a cross when it failed, outlined while it waits or runs, amber with a dot when it couldn't be tested. Hover it for what it means; click it to see the last lines the test printed, with Copy (for tests run since gitcraft started: a result from before, or from git test, has no output to show). Right-click the column header (or Settings → Columns) to show or hide the lane; the flask over it opens Test commits….",
            "A banner over the graph shows how far the run got, with Stop (tests that finished keep their result), then how it went and the oldest commit that failed (a click selects it).",
            "Results are kept by tree in git notes, as git-test keeps them (refs/notes/tests/default, the command in test.default.command), so git test on the command line and gitcraft share them. A rebase that keeps a commit's files keeps its square, and a tree tested once is not tested again unless you tick Test again commits that have a result. Changing the command can forget the old one's results.",
        ],
    ),
    (
        "Rehearse",
        &[
            "Right-click a branch (its pill, or in the left pane) and choose Rehearse merge or rebase… (or use the palette for the base branch) to try a rebase of the checked-out branch onto it, or a merge of it, before doing it. It is played through in a scratch worktree on the repository's machine; your branches and files stay as they are.",
            "The report says where it stops (for a rebase, at which commit, say commit 3 of 5) and which files conflict, or that it goes through; with a test command it also says whether the result passes (a result already stored for its files is taken instead of running it again).",
            "Tick Rehearse the merge too (or the rebase) to see both side by side, with whether they give the same files. Rebase now or Merge now does the one you pick for real, after checking that neither branch moved since; it goes into the undo history (Ctrl+Z), and conflicts open in the conflict editor as usual.",
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
            "Settings → Commit signing sets the format, key and program git signs with, and whether it signs commits and tags by default, in your global git config; Settings → Profile can sign just for the profile instead.",
            "Double-click a commit's message in the right pane to edit it (any commit of the checked-out branch with no merges after it).",
            "Undo and redo commits, merges, rebases, resets, checkouts, branches, tags and stashes.",
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
