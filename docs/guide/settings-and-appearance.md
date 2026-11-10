# Settings and appearance

Settings, themes, the title bar, commit signing, profiles and accessibility.

## Settings

- Settings (Ctrl+, or the gear in the title bar), in pages listed on the left: the profile in
  use and what is its own, zoom, branch column width, graph columns, avatars, notifications,
  update check (with a Check for updates button), background fetch, keeping submodules up to
  date, editor command, external diff and merge tools, default shell and terminal font size, saved identities, custom commands,
  SSH sign-ins, agents, the AI tool and its prompts, and every keyboard shortcut. Ctrl+F searches all pages at once (Esc clears it); Alt+↑ /
  Alt+↓ go through the pages.

## Previews

- Previews beside the settings: point at a setting and a panel on the right (above the page in
  a narrow window) draws what it does with the values set now, as the app draws it. The branch
  column measured on a small graph with long branch names cut to fit, the columns at their
  widths, standard and compact lanes side by side, the lane colours, each way of pulling as a
  before-and-after graph, the terminal with the shell's prompt at its font size, unified and
  side-by-side diffs, context or the whole file, files in folders or in one list, code colours,
  avatars, and the whole window in a theme.

## Title bar

- Tabs in the title bar (Windows and macOS, Settings → Appearance, on by default): the tab strip
  takes the place of the system's title bar, as in a browser. Drag the space next to the tabs to move the
  window (it snaps to the screen's edges as usual), double-click it to maximize; minimize, maximize and
  close sit at the end of the strip, drawn as Windows 11 draws them, and the window's edges
  resize it.
- The settings, command palette, theme and manual buttons sit in the title bar, or in the footer
  of a repository's left or right panel, in any order: drag one there (it snaps into the opening
  made for it), right-click it, or pick in Settings → Appearance. On a folded panel they stack up
  its strip. Tabs that don't fit scroll sideways under the end of the strip (wheel or touchpad),
  with `+` and a list of those out of view next to a grip that moves the window; when the title
  bar gets too narrow its buttons go into the profile menu.

## Themes

- Themes (Settings → Appearance, the palette icon in the title bar, or _Theme:_ in the command
  palette), each profile its own:
  - 40 built in: gitcraft's Dark, Medium Gray and Light (PhotoCraft's Pro palettes), High
    Contrast Dark and Light, and the published palettes of Catppuccin (Mocha, Macchiato, Frappé,
    Latte), Tokyo Night (Night, Storm, Day), Rosé Pine (main, Moon, Dawn), Kanagawa (Wave,
    Dragon, Lotus), Everforest (Dark, Light), Dracula, Nord, Gruvbox (Dark, Light), Solarized
    (Dark, Light), One Dark and One Light, GitHub (Dark, Light), Ayu (Dark, Mirage, Light),
    Monokai, Monokai Pro, Night Owl, Material Palenight, Tomorrow Night, Horizon and
    SynthWave '84. A theme colours everything: panels, text, the graph's lanes and pills, diffs
    and the terminal's 16 colours. WCAG AA checks (4.5:1) cover built-in text colours on their
    panel and row backgrounds, text-field hints, graph descriptions and status badges. Button
    and branch-pill labels are checked against their fills. Accent text is separate from the
    button fill so both remain readable. Status tints adapt to the
    selected or hovered background. Imported themes and custom colour values keep their
    author's choices; derived pill shading can change to keep white labels readable.
  - A gallery of thumbnails; pointing at one shows the whole window in it before choosing.
  - _Follow VS Code_: the interface takes the workbench colours of the theme VS Code (or
    Insiders, VSCodium) is using, and changes with it.
  - Themes from the internet: _Import theme file…_ takes a VS Code colour theme (`.json`, from an
    extension's `themes` folder or sites such as vscodethemes.com), a Base16 or Base24 scheme
    (`.yaml`, e.g. the tinted-theming collection) or a theme saved by gitcraft, and copies it
    into gitcraft's `themes` folder.
  - Your own: _Save a copy to edit_ writes the theme in use, every colour by name, as a file in
    that folder (`extends` names a theme to start from, so a file can set only an accent). Edit
    it in any editor and the window follows as you save; a file that doesn't read says why
    under the buttons.
  - For any theme: your own accent colour, rounder or square corners, and the graph's lanes in
    the theme's hues, gitcraft's classic ones or colour-blind-safe ones (Okabe–Ito).

## Avatars

- Avatars: on a GitHub repository, the authors' GitHub accounts (through `gh` when installed, else
  GitHub's API); on Forgejo and Gitea, the accounts the host ties the commits to; on GitLab, the
  host's avatar API; elsewhere Gravatar (a hash of the email).
  Cached on disk; can be turned off.

## Notifications

- Toasts, and a desktop notification when a long operation ends while the window is in the
  background.

## Commit signing and profiles

- Commit signing in Settings → Commit signing: the format (OpenPGP, SSH or X.509), signing key
  and program, and whether commits and tags are signed by default. These are your global git
  config's `gpg.format`, `user.signingkey`, `gpg.program` (`gpg.ssh.program`, `gpg.x509.program`),
  `commit.gpgSign` and `tag.gpgSign`, shown as they are set and written back there; git does
  the signing as usual. A profile can sign its own way instead (Settings → Profile).
- How a profile's git author and signing reach git: gitcraft writes them to a config file of the
  profile's (in its data folder, `profiles/<id>.gitconfig`) that includes your own global config
  first and then sets the profile's values, and runs git with `GIT_CONFIG_GLOBAL` pointing at it
  (git 2.32 or later). So the profile wins over `~/.gitconfig`, and a repository's own config
  (`git config --local`, `--worktree`) wins over the profile, as it does over `~/.gitconfig`.
  `git config --global` from gitcraft still reads and writes your own file. The built-in
  terminal keeps your own config.

## Keyboard, screen readers and reduced motion

- Keyboard and screen readers: Tab / Shift+Tab move through the toolbar, the sidebar's rows, the
  file lists, the graph, the tabs and the start page's cards; a painted button or row with the
  keyboard is ringed in the accent colour (egui's own buttons and fields show their pressed
  look), and Enter or Space clicks it. The graph is one Tab stop on the selected commit, ↑ / ↓
  moving the keyboard along. Each painted widget has a name for screen readers (AccessKit): a
  graph row reads "commit a301da9, Merge…, Jonas, 26 minutes ago, branch dev". Right-click
  menus and double-clicks (checking out a pill) still need the mouse.
- Reduced motion: when gitcraft starts it asks the system whether to animate (GNOME's
  `enable-animations`, KDE's animation speed, macOS' Reduce motion, Windows' "Animate windows
  when minimizing and maximizing"); with animations off, tabs and panels move into place without
  sliding. `GITCRAFT_REDUCE_MOTION=1` (or `0`) decides instead.
