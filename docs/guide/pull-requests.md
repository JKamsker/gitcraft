# Pull requests, issues and forges

Pull and merge requests on GitHub, GitLab, Forgejo (Codeberg) and Gitea: list, review, comment, merge, start work from an issue, and publish stacks.

## Setting up forges

- **GitHub** needs the [GitHub CLI](https://cli.github.com) (`gh`), signed in (`gh auth login`).
- **GitLab** needs the [GitLab CLI](https://gitlab.com/gitlab-org/cli) (`glab`), signed in
  (`glab auth login`).
- **Forgejo and Gitea** (Codeberg is Forgejo) need no tool of their own. gitcraft uses a login
  you already have for the host, and never stores one: `GITEA_SERVER_URL` +
  `GITEA_SERVER_TOKEN` when they name that server, else a login of `tea` (`tea login add`, which
  works with Forgejo too), else what git's credential helper keeps for `https://host` (a
  password or an access token). Public repositories can be read without any.

codeberg.org and hosts named like `forgejo…` / `gitea…` are recognised, as are the hosts `tea` is
logged in to. For any other server, tell git's config which forge it is:

```bash
git config --global gitcraft.git.example.org.forge forgejo   # every repository on that host
git config gitcraft.forge gitea                               # just this repository
```

The value is one of `github`, `gitlab`, `forgejo` or `gitea`.

## Primary forge remote

In **Remotes**, right-click a remote (or use its actions button) and choose
_Set as primary forge_. The starred remote supplies pull requests, issue lookups,
CI and default commit/file web links. This preference is kept per repository,
including across worktrees. Selecting it does not change Git's push/fetch settings
or branch upstreams. Explicit branch web links still follow that branch's remote.

_Use automatic forge selection_ restores the default: a recognised `origin`, else the
first recognised remote. Recognition uses local URL/config information, so choosing
works offline too. A unique rename with the same URL preserves the choice; removing
the selected remote restores automatic selection. Changing its URL follows the new
repository and clears old forge views so identical PR/issue numbers cannot mix.

## Pull requests

- Pull / merge requests of GitHub (through `gh`), GitLab (through `glab`), Forgejo and Gitea
  (through their API; Codeberg is Forgejo): list, open in the browser, check out, create from a
  branch. Click a request to focus its branch pill and show its full patch since the merge base.
  Double-click opens its details (in the browser for GitLab). On GitHub, Forgejo and Gitea
  the details open in the big area: description, checks,
  commits, conversation and changed files; comment, review (with comments on lines, pending until
  sent), reply, apply suggested changes, dismiss reviews and merge. Forgejo's and Gitea's API can't
  resolve conversations, so that stays on their web page. _Open in worktree_ (in a request's menu,
  and above the changed files of an open pull request) opens the worktree its branch is checked
  out in, or makes one beside the repository first: on the local branch, on a new one tracking
  the remote branch, or fetched from the forge for a fork that isn't a remote here.

## Start work from an issue

- Start work from an issue: _Start work from an issue…_ (the palette, or the button on the pull
  requests' header) lists the open issues of the repository assigned to you (`gh issue list
  --assignee @me`, `glab issue list --assignee=@me`, Forgejo's and Gitea's API with your login
  for the host; the first 100). Pick one, and gitcraft makes a branch named after it
  (`42-fix-login-crash`) from the remote's default branch (where its `HEAD` points, else main,
  master or develop), fetched first so that it starts where the remote is now, checked out here
  or in a new worktree beside the repository. A branch of that name already there is used as it
  is, and one only the remote has is tracked. The picker says beforehand which of these it does,
  and when changes in the working tree would go along. While that branch is checked out, a card
  above the graph keeps the issue in view: its _Acceptance criteria_ section, else its task
  list, else its description, as of the day it was read (read it again with a click), folded
  away with another. Which issue a branch is for is kept in the repository's git config
  (`branch.<name>.gitcraft-issue`): every worktree of the clone sees it, over WSL and SSH too,
  and git renames or deletes it with the branch; it isn't pushed. _Create pull request…_ (merge
  request, on GitLab) on that branch starts with the issue's title and `Closes #42`, into the
  branch it started from.

## What changed since you last looked

- What changed since you last looked: gitcraft remembers which version of each pull request you
  looked at (its page opened for the first time, or its comparison shown), in refs of the repository (`refs/gitcraft/reviewed/…`, which also keep its commits
  here once the forge no longer has them). After the author pushes again, the page says
  _Rewritten since you last looked: 2 commits changed, 1 added, 3 identical_ (or _New commits_
  when they were only added to); click it to compare the two versions, or ✕ to take the new one
  as seen. In the list (every forge, GitLab too) a request that changed says _changed since you
  last looked_, and its menu has _What changed since you last looked_ and _Mark as seen_. A
  request that only moved onto a newer base, every commit the same, says so quietly.

## Pull requests from forks

- Pull requests from forks (GitHub, GitLab, Forgejo, Gitea): the list marks those with workflow
  runs awaiting a maintainer's approval (GitHub; _Approve and run_ in the menu, chosen twice, as
  it runs the contributor's code on your CI), those whose author lets maintainers push to their
  branch (_can edit_) and first-time contributors (GitHub, GitLab). _Check out to push to ann_
  (in the menu, and on the pull request's page) adds the fork as a remote named after its owner
  (or uses the remote that already is it, by its URL), reached like `origin` (SSH or HTTPS),
  fetches the branch, checks it out tracking it and sets `branch.<name>.pushRemote`, so that
  Push sends your fixes back to the contributor's branch. When the branch's name is taken here
  it is checked out as `ann/branch`, and Push sends it to theirs by name. A remote added this
  way is marked `remote.<name>.gitcraftFork`. Ctrl+Z goes back, deleting the branch and the
  remote it added (refused once the branch has commits of yours). _Rebase onto main and push to
  ann_ fetches the base and their branch, refuses if they pushed commits you haven't, rebases,
  then force-pushes to their fork with a lease, after showing what it will do; if the rebase
  stops on conflicts, the push is offered once it is continued. Ctrl+Z puts the branch back on
  their fork and here. _Remove unused fork remotes_ (in the palette when there are some) takes
  out the marked remotes no branch uses any more, keeping those whose branches hold commits no
  branch here has (Ctrl+Z adds them back).

## Stacked pull requests

- Stacked pull requests, on GitHub, GitLab, Forgejo and Gitea alike: right-click a stack's
  title in STACKS (or _Publish its stack's pull requests…_ in a layer's menu). Each layer's own
  pull request is asked of the forge by its branch, whatever its state (never a fork's branch of
  the same name). The preview shows each one's base (the layer below; the bottom one goes into
  the branch the stack is on), its title (editable), its commits and `--shortstat` with a button
  for its changes in full, and whether it will be opened, retargeted or just pushed. _Publish_
  pushes each branch bottom-up (`--force-with-lease --force-if-includes`, git 2.30+: a restacked
  branch goes over its old commits, never over a teammate's fetched but not taken in) and opens
  the missing ones, each description naming the one it depends on, optionally as drafts. Publish
  again later to update them all: pushed again, and any whose base is wrong pointed at the right
  one. When a lower one is merged, it is left alone and the ones above go into what it went
  into, also as recorded in git's config (restack them afterwards); one closed without merging
  is opened anew. A layer with nothing of its own is pushed, with no pull request. Each one's
  number is recorded on its branch (`branch.<name>.gitcraft-pr`) as soon as it is known, and a
  publish that failed halfway is run again without opening any twice.

## CODEOWNERS

- CODEOWNERS: the file at HEAD, found where the forge of `origin` looks for it (GitHub:
  `.github/`, the root, `docs/`; GitLab: the root, `docs/`, `.gitlab/`; Gitea: the root, `docs/`,
  `.gitea/`, and on Forgejo `.forgejo/` after those) and read its way: on GitHub,
  gitignore-like patterns where the last match wins; on GitLab, patterns anchored only by a
  leading `/` and matching files only (a trailing `/` for a folder's contents), in sections
  (`[Docs]`, `^[Optional]`, `[Approvals][2] @owner`, those of one name merged) whose owners add
  up, where an exclusion (`!pattern`) that matches drops the section; on Gitea and Forgejo,
  regular expressions on the whole path (`^pattern$`), where every match adds its owners and
  `!regex` stands for the other files. Each file row (changes, a commit, a comparison, a pull
  request) shows its owners; a line above the files of a commit, comparison or pull request,
  and above the staged files, says who they need ("Needs payments and infra · 2 files without
  an owner"), worked out in the background. _Create pull request_ suggests, ticked, the owners
  of what the branch changes since its base, read from the base's CODEOWNERS as the forge reads
  it (none for a draft). Once it is open they are asked one at a time (GitHub: accounts and
  teams; GitLab: accounts; Forgejo and Gitea: both), so one the forge refuses — the author, an
  account that is gone — is said and doesn't keep the others from being asked. _Check
  CODEOWNERS_ in the palette lists the lines the forge would skip or read differently, the
  patterns that match no file and the files without an owner.
