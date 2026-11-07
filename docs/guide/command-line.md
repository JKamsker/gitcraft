# Command line, file manager and links

Starting gitcraft from a terminal, a file manager's right-click menu, or a link in the browser.

## From the terminal

- `gitcraft .` (or `gitcraft <folder>`) opens the repository that folder is in, from any
  subfolder or linked worktree, also a `\\wsl.localhost\…` folder.
- `gitcraft log <file>` opens a file's history (also of a file deleted since), `gitcraft blame
  <file>:<line>` its blame scrolled to and marking that line (`gitcraft blame -- <file>` takes a
  name ending in `:12` as it is), `gitcraft show <rev>` selects a commit of the current folder's
  repository, and `gitcraft compare a..b` lists what differs between two commits (`a...b`: what
  `b` changed since they diverged). `gitcraft --help` lists them; a revision that starts with `-`
  is refused.

## From the file manager

- _History in gitcraft_ and _Blame in gitcraft_ on files: the Windows installer adds them to
  Explorer's right-click menu (an option, like _Open in gitcraft_ on folders); the .deb and .rpm
  packages add a Dolphin service menu (`packaging/linux/gitcraft-servicemenu.desktop`, also in
  the `.tar.gz`); Settings → General adds them to Files (GNOME) as scripts and to Finder as
  Quick Actions (they may show after logging in again). Each starts gitcraft, which hands the
  file to the window already open. No overlay icons.

## Links from the browser (`gitcraft://`)

- Links from the browser: `gitcraft://open?url=<forge link>` opens that pull request, commit or
  file (`#L40` scrolls the blame to the line) in your clone of the repository, found by its
  remotes' URLs among the open tabs and the recent repositories; with none, the clone dialog
  opens with the repository's URL (clone, then open the link again). A link to a pull request opens in a clone whose `origin` is that repository (a fork's `upstream` isn't where its pull requests come from); a file link opens at the linked commit when the clone has it, else as the clone has the file, and a branch with `/` in its name is told from the folders by what the clone has. The link itself pasted into the palette (Ctrl+P) or the start
  page's field, or given as `gitcraft <link>`, does the same. GitHub, GitLab (groups too),
  Gitea / Forgejo (Codeberg) and Bitbucket links are read by their paths; every link is checked
  (https or http only, a host name with a dot such as `git.example.com`, so not `localhost`, no `..`, a revision that can't be an option) and
  anything else is refused with why. The Windows installer registers the `gitcraft://` scheme
  (an option), the .deb / .rpm install `gitcraft-link.desktop` (`x-scheme-handler/gitcraft`),
  and the AppImage's menu entry adds one. macOS hands a link to an app as an Apple Event, which
  the windowing library doesn't pass on, so there paste the link instead.
- Without a terminal (a menu, a file manager, Windows), what the command line says (`--help`,
  a mistake) shows in a dialog.
