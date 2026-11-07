# Diffs

The centre of the window: unified or side-by-side diffs, pictures, notebooks, binary files, and readings that understand the file.

## Basics

- Unified or side by side (a new file takes the whole width, with no empty old side), syntax
  colours, the changed part of each line marked, whitespace changes optional, images shown
  before / after. Esc closes.

## Pictures

- Pictures compare four ways: _2-up_ side by side, _Swipe_ (drag the line, or the slider),
  _Onion skin_ (the new one over the old, as see-through as the slider says) and _Difference_
  (the pixels that changed, bright over a faint old picture). Zoom (the wheel, at the pointer)
  and pan (drag) stay the same across the modes and both sides of 2-up, the two pictures laid
  out on one canvas so that pixels line up; _Fit_, _100%_ and a double-click reset them. A
  checkerboard shows through transparent parts, and the header says how the dimensions and
  size changed and how many pixels did. Pictures past 4096 pixels are shown scaled (both by the
  same factor); a picture that doesn't decode (an `.ico`) shows before / after as before.
  Below, the picture's history is a strip of thumbnails, newest first; click one to see that
  commit's change.

## Notebooks, Git LFS, Unity and binary files

- Jupyter notebooks (`.ipynb`) diff cell by cell: each changed cell's source line by line, its
  outputs (text, errors, pictures; HTML and SVG as their source, long ones cut) before and
  after, a changed metadata shown line by line, a new execution count only noted, unchanged
  cells folded into a line; cells matched by their ids, or by their sources in order, and moved
  ones marked. Nothing in a notebook is run. _JSON_ switches back to the file's own lines (to
  stage a part of them). _Strip outputs when committing_ sets a clean filter in this clone only
  (`.git/config`, not shared with others), using nbstripout when it is installed, else a small
  Python 3 script: what is staged has no outputs, execution counts or widget state while the
  file on disk keeps them, so a notebook whose outputs alone changed no longer shows as
  modified. A filter set by another tool is shown and left alone.
- A file kept in Git LFS shows as its LFS objects before and after, with their sizes, rather
  than the text of the pointer git stores. With git-lfs installed, the palette (Ctrl+P) of a
  repository that uses LFS has _LFS: Fetch_ and _LFS: Pull_.
- LFS file locks: a padlock on a file's row shows who holds its lock (hover it for since when);
  _Lock_ and _Unlock_ are in the menu of a file of the working tree or the checked-out commit.
  _LFS: Locks…_ in the palette lists every lock, filters them by path or owner, unlocks several
  at once, force-unlocks someone else's (admins only) and makes a pattern lockable
  (`git lfs track --lockable`; commit `.gitattributes` to share it). Above the changes, a
  changed file someone else holds is flagged (the push would be refused), as are changed
  lockable files nobody holds, with _Lock all_. After a push of the checked-out branch,
  gitcraft offers to give back your locks on the files it carried that have no changes left.
  Locks are read when the tab opens and every few minutes while background fetching is on
  (Settings), and whenever the panel opens.
- LFS storage: _LFS: Storage…_ in the palette of a repository that uses LFS says how much disk
  the local LFS objects take, lists the files of the checkout that are only their pointer on
  disk (never downloaded, or fetched but not checked out) with _Download_ for each or all, sets
  which folders git-lfs downloads on fetch, pull and checkout (`lfs.fetchinclude` /
  `lfs.fetchexclude`, for this repository only; what a committed `.lfsconfig` sets is shown),
  and prunes old objects after showing what that frees, asking twice. In _View all files_ of
  the checked-out commit, a cloud marks the files that are only pointers (not on files that
  commit changed, whose stats take that place). The menu of a file of the checkout has _LFS:
  Download_ for such a file, _LFS: Fetch only <folder>_ for a file in LFS, and _Track \*.ext in
  LFS_ for one whose type isn't (which writes `.gitattributes`).
- Unity projects (at the root or one folder down) are recognised, before the first commit too.
  _Unity: Set-up…_ in the palette checks that assets are saved as text (Force Text) and `.meta`
  files are visible, that `.gitattributes` merges Unity's YAML with Smart Merge (and, in a
  repository that uses LFS, keeps its binaries there), that `Library`, `Temp`, `Logs` and the
  other generated folders are ignored and not committed, and adds the missing lines or stops
  tracking those folders on request. A hint above the changes names the first thing worth
  fixing (_Don't show again_ turns it off for the repository). It finds UnityYAMLMerge in Unity
  Hub's editors (or one you choose) and registers it as the repository's merge driver; when it
  is found, a conflicted scene or prefab has _Solve with UnityYAMLMerge_ in the conflict editor,
  which shows the result before saving. Above the changes, an asset or folder staged without
  its `.meta` (or a `.meta` without its asset, or one deleted without the other) is flagged,
  with a button to stage the missing half, delete the leftover `.meta` too, or keep the `.meta`.
- Other binary files show what they are instead of "Binary files differ": their kind (and a
  change of kind), what changed in size, and a before / after table of what can be told about
  them (a PDF's version, pages, title and author; an archive's members and unpacked size;
  the dimensions of a picture the image view doesn't take). Then _Text_, _Members_ or _Bytes_:
  the text of Word and OpenDocument files, the slides of PowerPoint files in the order they
  are shown, the cells of Excel sheets (a formula next to its value), and the text of PDFs
  where it is plainly in them, diffed line by line; the members of a ZIP-based file added,
  removed and changed; and the first 64 KB of both sides in hex, bytes that differ at the same
  offset marked. Files over 64 MB aren't read. A note says when a side's text is incomplete (a
  slide or sheet too big to read), when the texts are too unlike to match line by line, and
  when a side couldn't be read or its member list is damaged (its members then aren't
  compared). A file with a `textconv` driver shows git's text of it, said so above the diff,
  and is staged or unstaged as a whole.

## Moving around a diff

- The arrows in a diff's header (F7 / Shift+F7, or Alt+↓ / Alt+↑) jump to the next and previous
  change. The strip beside the scrollbar marks where the changes are, removals red and additions
  green; click a mark to jump to its change, or anywhere else on the strip to scroll there.
- The panel button in a diff's header (or Settings → Graph → Diffs) puts the diff below the graph
  instead of in its place (remembered): the graph stays usable, selecting another commit keeps
  the diff, which follows the next file opened, and the border between them drags to resize.
  Diffs of a file's history, of a pull request, over the builds, and the conflict editor keep
  their own place.
- Context that opens where you need it: each hunk's header says how many unchanged lines are
  folded above it (_42 unchanged lines_), and a row after the last hunk those below it. _↑ 20_
  shows the lines right above the hunk, _↓ 20_ those right below the hunk before, _Show all_
  every one, and _Function_ the lines of the function git names for the hunk (when it encloses
  the hunk), up to its first line and down to its end. In a narrow pane the least needed
  buttons give way first. The lines opened join their hunk as context, so staging, unstaging
  and discarding hunks or picked lines work as before; those next to a hunk stay open when the
  diff is read again (after staging, or a change on disk), and a line a pull request's review
  commented on opens by itself. Folds are only offered when the file read matches what git
  diffed: not after a clean filter (`core.autocrlf`, Git LFS) or textconv, nor for files that
  aren't UTF-8. Close and reopen the diff to fold everything again.
- Moved code: a block removed in one place of the file and added in another (the whitespace
  around its lines aside, so re-indenting doesn't hide it) shows in a muted colour of its own
  instead of red and green, with a bar beside both places. _Moved to line…_ / _Moved from
  line…_ on its first line opens a panel with the two places together: the lines kept, those
  edited on the way (the changed part of each marked), and buttons to go to either place; the
  button reads _…, edited_ when lines changed on the way. Esc or its × closes the panel.
  Staging hunks and lines works as before (a click on the button opens the panel rather than
  picking that line).
- The _Diff_ / _File_ switch shows the whole file as it is after the change instead (before it,
  for a deleted file), coloured and with line numbers.
- Ctrl+F in a diff (unified or side by side), a file or a blame finds in it rather than in the
  commits: every match is marked and _Enter_ / _Shift+Enter_ (or F3 / Shift+F3) step through
  them in the order of the rows; the bar shows which match of how many (e.g. 3 of 12). Match
  case, whole words and regular expressions are toggles; Esc closes the bar. A very long file
  is searched a few lines per frame, so the window stays responsive.
- The algorithm in a diff's header (_Default_, _Myers_, _Minimal_, _Patience_, _Histogram_)
  pairs the lines another way when they line up badly: the diff is read again with
  `--diff-algorithm`, the line at the top of the view stays at the top, and the choice is kept
  for that file type (`*.rs`, `makefile`…; Settings → Git → Diffs lists and forgets them). The
  repository's config is never written; _Default_ is what `diff.algorithm` says.

## Generated files and lockfiles

- Generated files and lockfiles start folded: files marked `linguist-generated` or `-diff` in
  `.gitattributes` (read with `git check-attr`, so every attributes file counts, and
  `linguist-generated=false` keeps a file open), lockfiles (`Cargo.lock`, `package-lock.json`,
  `yarn.lock`, `pnpm-lock.yaml`, `poetry.lock`, `go.sum`, `Gemfile.lock`, `composer.lock`…) and
  minified files (`*.min.js`, `*.min.css`). File lists gather them at their end under one
  closed row (_2 lockfiles +1,204 −880_; the working tree's lists have no counts), and a file
  clicked there opens. Reached another way (its history, a pull request's timeline), its diff
  shows only a line such as _+1,204 −880, lockfile_ until _Show the changes_. Settings → Git → Diffs adds patterns of your
  own (`*.snap`, `dist/**`, `vendor/`) or turns folding off.

## Readings that understand the file

- Readings that understand the file: some files get a button in the diff's header that shows
  their change in a way of their own instead of as lines (click it again for the lines, where
  staging stays). They read both sides of the file in the background (on the repository's
  machine, for WSL and SSH, under its old name before a rename); a click on a part goes back
  to the lines, at its line or the nearest one the diff shows, and a file they can't read (not UTF-8, over 8 MB, malformed) shows the plain diff with a note
  saying why.
  - _Prose_ (the book, for `.md`, `.adoc`, `.rst` and `.txt`): paragraphs wrapped to the
    pane. In an edited paragraph the removed words are struck through on red and the added
    ones on green, compared word by word (a punctuation mark counts as a word of its own;
    the text keeps its own spacing), so a paragraph only re-wrapped says _re-wrapped_ and
    marks nothing. Removed and added paragraphs show whole; long runs of unchanged ones fold
    (click to open).
  - _By key_ (the list, for JSON, YAML and TOML): one row per key path that changed, such as
    `server.timeout: 30 → 60`, keys added and removed, a value that became another kind
    (`boolean → string`), and keys that only moved within their map listed apart from
    changed values (_moved, same value_). List items are matched by content, so an item
    inserted at the top doesn't make every one after it changed (in lists of a few thousand
    items that differ; longer ones are compared item by item in place). Comments and
    formatting don't count (quoting does in YAML, where `'5'` is a string and `5` a number);
    a YAML alias is compared as what its anchor holds; JSON may have the comments and
    trailing commas of JSON-with-comments files. A file that doesn't parse says on which side
    and line.
  - _As a table_ (for `.csv` and `.tsv`): a grid of the rows added, removed and changed, the
    changed cells as `old → new`, the unchanged rows between them folded (click to open) and
    the header kept on top. Columns are matched by name; one renamed (most of its values the
    same) shows `old → new name`, and columns added or removed are marked. Rows are matched
    by their first column when its values are all there and all different (a key), else by
    content. Quoted cells may hold commas, quotes and line breaks; a quote that never ends
    falls back to the plain diff with the line.
  - _Data tracked by DVC_ (for `.dvc` files): each path it tracks before and after, with its
    size, hash and, for a folder, how many files it holds, as files in Git LFS show.
  - _Dependency changes_ (the package, for `Cargo.lock`, `package-lock.json`,
    `npm-shrinkwrap.json`, `pnpm-lock.yaml`, `poetry.lock`, `uv.lock`, `composer.lock` and
    `go.sum`): the packages upgraded, downgraded, added and removed, each with its versions,
    a link to its registry page (crates.io, npm, PyPI, Packagist, pkg.go.dev) and one to what
    changed between the two versions (diff.rs for crates, npmdiff.dev for npm packages, the
    release history elsewhere); changes of licence are shown where the lockfile records them
    (npm, Composer). A version installed in several places counts once; a package kept at
    several versions is listed per version added or removed; a git dependency or a Composer
    branch shows its commit, so moving it is seen; `go.sum` is read as the newest version of
    each module (it keeps lines of older ones). The project's own packages (workspace members,
    path dependencies) get no registry links. A click on a package's name goes to its line. The links open in the browser only
    when clicked: reading the lockfile asks nothing of any server.
  - _API contract changes_ (the server, for an OpenAPI or Swagger file: offered on files named
    like one, and from the view by key of any JSON or YAML file that is one): what breaks the
    API's clients, each with its endpoint — endpoints removed, parameters that became required
    or are new and required, a request body or request field now required (read-only fields
    aside), a request media type dropped, request values narrowed (an enum value taken away,
    no more other fields), types that no longer take what they took (`integer` → `number` is
    wider, not breaking), success responses removed, response fields that were always there
    removed or no longer always returned, and responses that can now return an enum value
    clients don't know — then the endpoints added. A path parameter renamed is the same
    endpoint; header names are compared in any case. Local `$ref`s are followed, each pair
    once per endpoint (schemas that refer to each other end). A click opens the change's
    place in the file.
- A Markdown file's diff has an eye button that renders it: the changes block by block (removed
  ones red, added ones green, the rest as they are) or, with _File_, the whole file. Images of
  the repository show as they are on each side of the change; any other image is a link, so a
  file can't make gitcraft contact a server.

## Weakened checks and code colours

- _weakens checks_: a file of a commit, a comparison or the changes (staged, unstaged and untracked
  apart), and each hunk of its diff that does it, gets this badge when it removes assertions or
  tests (not ones moved to another file), deletes a test file, skips, disables or focuses tests
  (`#[ignore]`, `@pytest.mark.skip`, `it.skip`, `xit`, `.only`, `@Disabled`, `[Ignore]`,
  `t.Skip()`…), adds an assertion that can't fail (`assert True`), loosens a tolerance in a test
  (`rtol=1e-6` → `1e-2`), rewrites a snapshot wholesale, lets CI fail (`continue-on-error`,
  `allow_failure`, `|| true`), removes a GitHub Actions / Forgejo / Gitea or GitLab job or
  trigger, or adds `--no-verify`. Fixed rules on the diff's lines and the files' names, the same
  way every time (prose files and text in strings left out); the badge's hover lists the lines
  that made it, `old line` for removed ones.
- Code takes the colours of the theme VS Code (or Insiders, VSCodium) is using, read from its
  settings and extensions, when it is dark or light like gitcraft's; a setting turns it off.
