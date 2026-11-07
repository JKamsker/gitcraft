# Releases and packages

Cutting a release, git-flow style hotfixes, and checking what a package will publish.

## Cut a release

- _Cut release…_ (a commit's menu, or the palette for HEAD) suggests the next version from the
  Conventional Commits since the last version tag (`v1.2.3` or `1.2.3`; other tags are passed
  over): `feat` moves the minor version, `!` or `BREAKING CHANGE` the major one (the minor one
  before 1.0), anything else the patch; a release candidate becomes its release; the tag's `v`
  (or its absence) is kept. It drafts the notes, grouped by type (breaking changes, features, bug
  fixes, performance, documentation, other), with links to the pull requests their summaries
  name (`(#12)`), to edit. It can move the version in `Cargo.toml` (and the package's entry in
  `Cargo.lock`) and `package.json` (and `package-lock.json`), changing nothing else in them, and
  add the notes on top of `CHANGELOG.md`, in one commit, `chore(release): v1.3.0` (from the tip
  of the branch checked out, with those files unchanged); make an annotated or signed tag whose
  message is the notes (headings kept); push that commit to the branch's upstream and the tag,
  both or neither (`--atomic`); and make the release on the forge (`gh release create`,
  `glab release create`, Forgejo's and Gitea's API; pre-releases except on GitLab) with files
  of this computer attached. The dialog says what each step will do before anything runs, then
  how each went. A step that failed (a push refused, the forge unreachable) can be resumed from
  there — a release or file the forge has already is kept, not made twice — or skipped; a push
  that needs signing in asks. Ctrl+Z takes the version commit and the tag back off here (the
  tag put back as it was on Ctrl+Y); what was pushed and published stays.

## Hotfixes

- Hotfixes, the git-flow way: _Start hotfix from v2.3.1…_ in a version tag's menu offers a branch
  `hotfix/2.3.2` there (git-flow's prefix when it is set up), checked out. _Finish hotfix…_ (the
  branch's menu, or the palette while it is checked out) shows the tag it will make (`v2.3.2`,
  with notes from its commits to edit) and what it will do, then, as one step: merges it into the
  development branch (`develop` or git-flow's, if there is one) and the main one (`main`,
  `master` or git-flow's) with `--no-ff`, tags main's merge (annotated, or signed when ticked;
  git-flow's tag prefix when it is set up), deletes it here, and pushes those branches (to their
  upstreams) and the tag at once (`--atomic`). A merge that would stop on conflicts is said
  beforehand (git 2.38 or later) and refused, and so is a branch checked out in another
  worktree. Ctrl+Z puts the hotfix branch back (checked out, with its upstream), the branches
  where they were and takes the tag off here; what was pushed stays, and Ctrl+Y brings back the
  same merges and tag. Should it stop part-way (a checkout or a merge refused), Ctrl+Z puts back
  what it did.

## What a package will publish

- _What a package will publish_ (from the palette, Ctrl+P) lists the files
  `cargo package --list` or `npm pack --dry-run --ignore-scripts` would ship for each crate of
  the repository's Cargo workspace and its top `package.json`, run where the repository is
  (here, in WSL or over SSH). It flags what looks shipped by accident (`.env`, keys, logs, build
  output, editor folders, a crate's files git doesn't track, big files), what is left out that a package
  usually wants (the README and licence, a crate's `src/` files, the files the manifest names:
  `readme`, `main`, `bin`…), and, asked of the registry once the files are listed, the files new
  or gone since the version on crates.io or npm (not compared for a package published to
  another registry). Refresh reads it again after a change.
