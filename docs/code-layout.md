# Code layout

A map of where things live in `src/`, to find your way in. It isn't exhaustive: most features
have a module of their own named after them (`src/teleport/`, `src/parallel_bisect/`,
`src/repo/bisect/`…), and the logic usually sits apart from its UI (`src/<feature>/` for what it
reads and does, `src/repo/<feature>/` for how a repository tab shows it).

- `src/main.rs` starts the app, `src/lib.rs` holds everything else, so tests and examples can
  reach it.
- `crates/gitcraft-os` holds the few Windows system calls (taskbar progress, jump list) that need
  `unsafe`, so that gitcraft itself keeps `unsafe_code` denied.
- `examples/ui_shot.rs` drives the window headlessly (see [Development](development.md#looking-at-the-ui-headlessly)).
- `packaging/` and `.github/workflows/release.yml` build the installers and packages.

Assets and their licences are listed in [ATTRIBUTION.md](../ATTRIBUTION.md).

| File | Job |
| --- | --- |
| `src/git/` | `git` CLI wrapper and output parsing (refs, log, status, stashes, remotes, search) |
| `src/git/simulate/` | a merge, rebase or cherry-pick worked out with `git merge-tree`, without doing it |
| `src/git/errors.rs` | known git failures told apart by git's words; their fixes (`errors/fix.rs`, `errors/lock.rs`) and git's real output for tests (`errors/samples/`) |
| `src/error_card/` | the card that explains a git failure and offers its fix (confirmed, undoable) |
| `src/graph.rs` | lane layout for the commit graph |
| `src/forge/mod.rs` | the forges behind one `Provider` trait: which one hosts a remote, list, create, avatars, the page of one |
| `src/forge/github/` | GitHub through `gh`: list, `gh pr view`, GraphQL threads, comments, reviews, merges |
| `src/forge/gitlab.rs` | GitLab through `glab`: list, create, find by branch and retarget |
| `src/forge/gitea/` | Forgejo and Gitea through their REST API: list, the page of one, comments, reviews, merges; finding the login |
| `src/forge/stack.rs` | a stack's pull requests: planned, published bottom-up, retargeted, recorded |
| `src/forge/http.rs` | the REST client for forges without a command-line tool |
| `src/forge/cli.rs` | running `gh` / `glab` |
| `src/forge/links/` | links to commits, branches, files and lines for each forge; the remote they go to |
| `ci/forge-server.sh` | a throwaway Forgejo or Gitea for the live tests (`src/forge/gitea/live.rs`) |
| `src/pull_request/mod.rs` | one pull request in full, whatever the forge: details, checks, comments, changes, safe markdown |
| `src/pr_post/mod.rs` | what goes out: the `Target` trait each forge implements, and what they share (suggestions, merge rules) |
| `src/pr_threads.rs` | review threads, the pending review and permissions |
| `src/codeowners/` | CODEOWNERS of GitHub, GitLab and Gitea / Forgejo: where each looks, its patterns, who owns a file, the check |
| `src/repo/owners/` | CODEOWNERS in the tab: owners on file rows, who a change needs, reviewers to suggest, the check's window |
| `src/forge/forks.rs` | where each forge's pull requests come from (forks), what their authors allow, runs awaiting approval |
| `src/repo/forks/` | pull requests from forks: badges, checking out to push to the contributor, rebase and push, tidying fork remotes |
| `src/release/` | cutting a release: versions, Conventional Commits, notes, manifests, each step |
| `src/repo/release/` | the release dialog: choices, what it will do, the steps as they run, resume and undo |
| `src/repo/hotfix/` | the hotfix flow: starting one from a tag, finishing it (tag, merges, push) and its undo |
| `src/avatars.rs` | avatars from GitHub, Forgejo / Gitea, GitLab or Gravatar, cached on disk |
| `src/update/` | new-release check |
| `src/desktop_entry.rs` | applications menu entry for the AppImage |
| `src/shell_path.rs` | on macOS, the login shell's PATH for the commands gitcraft starts |
| `src/package/` | what a crate or npm package will publish: the tool's listing, what looks wrong, the registry's version |
| `src/repo/package/` | the panel that shows it |
| `src/remote/` | repositories in WSL or over SSH: their paths, the shells kept open there, their files |
| `src/agent/` | coding agents: the worktree setup recipe, the agent tasks, the terminal script that runs them |
| `src/repo/agents/` | the AGENTS section and the agent task and setup recipe dialogs |
| `src/agent/checkpoint.rs` | agent turns: `gitcraft checkpoint`, the turns of a worktree; `hooks.rs` installs the agents' hooks, `checkpoint.sh` is the script for other machines |
| `src/agent/handoff.rs` | hand-off: the messages typed into an agent's terminal (notes, conflicts, CI logs, threads), the notes tray |
| `src/agent/bakeoff/` | bake-off: comparing attempts, new dependencies, hunks, assembling a branch, builds |
| `src/repo/bakeoff/` | the bake-off page and its attempt picker |
| `src/app/title_bar.rs` | Windows and macOS: the tab strip as the title bar (moving, caption buttons, resizing) |
| `src/settings/` | user settings, profiles, the settings pages, the theme gallery and the previews |
| `src/ai/` | AI commit messages and reviews: the prompt and diff cut to size, finding and running the tool on the repository's machine |
| `src/repo/ai/` | the commit box's AI button and the branch review panel |
| `src/diagnostics.rs` | Help → Copy diagnostics: what a bug report needs, nothing personal |
| `src/manual.rs` | the manual (F1): features and keyboard shortcuts |
| `src/keymap.rs` | the keys of the main actions and tabs, read by handlers, manual, tooltips, menus |
| `src/menu_item.rs` | a menu entry with its key on the right and why it is greyed out under it |
| `src/repo/keys.rs` | what a repository tab's keys do, or why they can't now |
| `src/a11y/` | keyboard focus ring, screen-reader names of painted widgets, reduced motion |
| `src/theme/` | colour tokens, fonts, egui style; themes: catalogue, palettes, theme files |
| `src/icons.rs` | embedded Lucide SVG icons, tinted |
| `src/terminal/` | integrated terminal: pty, VT parsing (`vt100`), painting, keys, selection |
| `src/repo/terminals.rs`, `src/repo/terminals/` | terminal panel owner: private shell collection, selection/visibility/launch transitions, typed actions, palette contributions and tab strip |
| `src/repo/mod.rs` | one repository tab: state, background git work, undo / redo, toolbar |
| `src/mcp/` | `gitcraft mcp`: the MCP server (`rmcp`), its tools, and the token-checked channel to the window |
| `src/repo/agent/` | agents' requests in a tab: planned as undoable steps, queued, the approval card's state |
| `src/repo/timeline/` | the timeline and undo tree: entries kept on disk, changes seen made elsewhere, jumps, the refs that keep old tips |
| `src/repo/sidebar/` | branches, remotes, worktrees, stashes, pull requests, tags |
| `src/repo/pr_view/` | a pull request in the big area, its changed files on the right |
| `src/repo/pr_review/` | comments and reviews being written, and the threads on lines of a diff |
| `src/repo/pr_merge.rs` | the box that merges a pull request |
| `src/repo/push/` | Push and Sync; a refused push read from `--porcelain`, the remote checked, the banner and the force push with lease |
| `src/repo/pinned_sections.rs` | sidebar section headers that stay in view and jump to their section |
| `src/repo/branch_tree.rs` | branch names and file paths as a tree of folders, and its rows |
| `src/repo/branch_health.rs` | which local branches are merged (or rebased, or squash-merged), stale or gone |
| `src/repo/branch_cleanup.rs` | the branches _Select stale or merged branches…_ offers, read when the branches change |
| `src/repo/branch_order.rs` | pinned branches and the sort of _Local_, kept per repository |
| `src/repo/columns.rs` | widths of the graph's columns: what a drag changes and what gets squeezed |
| `src/repo/worktree_tab/` | worktrees in the same tab: parked views, the tab's chip and switcher |
| `src/repo/graph_view/` | commit graph, pills, search bar, commit menu |
| `src/repo/detail/` | commit / stash detail, changes and commit box |
| `src/repo/detail/extras/` | a commit's signature, containing branches and tags, notes and backlinks |
| `src/repo/diff/` | diff loading, pairing and the diff view (unified, split, images) |
| `src/repo/diff_split.rs` | the diff below the graph instead of in its place |
| `src/repo/nav_history.rs` | the back / forward list of a tab's recent views |
| `src/repo/navigation/` | back and forward: the view shown as a place, showing one again, keys and buttons |
| `src/repo/fold/` | generated files and lockfiles folded in file lists and diffs |
| `src/repo/highlight.rs` | syntax colours (syntect) |
| `src/vscode_theme/` | VS Code's current colour theme as a syntect theme |
| `src/repo/conflict/` | three-pane conflict editor, each conflict's original, Keep both |
| `src/repo/conflict_check.rs` | conflict warnings in the menus and the toolbar, checked in the background and kept |
| `src/git/external_tool/` | external diff and merge tools: presets, how git starts them (here, in WSL, over SSH), how a merge ended |
| `src/repo/doctor/` | the repository doctor: its page, sections and what they ask the tab to do |
| `src/git/doctor/` | what the doctor reads and changes: config origins, hidden flags, ignore rules and attributes, hooks and what they printed, health |
| `src/mergiraf/` | Mergiraf, run as the user's own command when it is installed |
| `src/repo/surgery/` | history surgery: splitting a commit (`split/`), moving changes (`moving.rs`), absorbing staged fixes (`absorb/`), patches of some lines (`patchset.rs`) |
| `src/git/trees.rs` | commits made in a scratch index (`tree_after`, `commit_tree`) for rewrites |
| `src/repo/rebase.rs` | interactive rebase editor and its todo list (`rebase/rows.rs`: its rows, drag, keys) |
| `src/repo/reflog/` | the reflog view and the scan for lost commits (`src/git/reflog/`: reading and parsing them) |
| `src/repo/drag_drop/` | drag and drop: what a drop offers, the drop menu, moving and squashing commits |
| `src/repo/dco/` | DCO sign-off: the commit box's toggle, commits without one, signing off a branch |
| `src/repo/versions/` | two versions of a series compared (range-diff), before / after rewrites, pull requests since last looked at |
| `src/git/range_diff.rs` | running and reading `git range-diff`; `rewrites.rs` and `reviewed.rs` keep the old versions in refs |
| `src/repo/file_view/` | a file's history (or a few lines'), blame and contents at a commit |
| `src/repo/revert_piece.rs` | reverting a file, hunk or lines of a past commit into the working tree |
| `src/git/stack/` | stacked branches: the parents recorded in git's config, each layer's state, the parent suggested from history |
| `src/repo/stack/` | stacks in the tab: the sidebar's STACKS, _This branch is based on…_, the graph's tooltips |
| `src/repo/file_text.rs` | a whole file at a commit, coloured, for the file view and diffs |
| `src/repo/find/` | find in the diff, file or blame shown: the bar, matching, marks, keys |
| `src/custom_commands/` | custom commands: what they are, the script each runs (its variables quoted), a repository's `.gitcraft/commands.json` and its trust |
| `src/repo/custom_commands/` | custom commands in a tab: menu and palette entries, the repository's file, running one (toast or terminal) |
| `src/repo/forge_link/` | the _Copy link to …_ / _Open … on GitHub_ menu entries, and the link made in the background |
| `src/repo/dialogs/` | branch, tag, stash, remote, worktree, pull request and confirm dialogs |
| `src/git/preview.rs` | what a destructive operation would change, read before it runs |
| `src/repo/widgets/` | shared painted widgets (panel headers, file rows, avatars, tabs) |
| `src/repo/palette/` | what the palette offers a repository: commands, the selection's, branch verbs |
| `src/repo/commit_actions.rs` | cherry-pick, revert and reset of a commit, for its menu and the palette |
| `src/app/` | window shell: tabs, start page, cloning, palette, settings, toasts |
| `src/app/palette/` | the command palette: scored matching, recently used entries, the second step |
