# Repository tour and portable setup

Open **A tour of this repository** from the command palette in a repository. It shows
README and contribution excerpts, observed history habits, branch/tag names, recent
folder contributors and hot files, and an interactive setup checklist. Observations are
not contribution policy: signature headers are not verified signatures, sign-off lines
are not proof of DCO compliance, and linear history cannot distinguish squash/rebase
from ordinary linear development.

The tour reuses Activity's immutable, canonicalized history snapshot when available.
Otherwise it reads at most 5,000 unique commits reachable from HEAD and project
heads/remotes/tags; stash-only and Gitcraft snapshot refs are excluded. The recent window
is 90 civil days by each commit's recorded committer timezone, ending on the current UTC
day. Partial/shallow history and scan limits are shown. Documents come from the recorded
HEAD tree, with no lazy fetching: at most 20 documents, 256 KiB each, and 6,000 displayed
characters per document. Missing objects remain unavailable. Refresh after source or
mailmap changes. Hook markers and tool needs are hints to review; project scripts are
never run by the tour. LFS installation and submodule initialization require explicit
user actions.

Use **Export setup…** on the start page or command palette to select recent/open
repositories, build a preview, then save one new JSON file. Version 1 uses the
`gitcraft-setup` format. The 8 MiB file contains allowlisted display settings, built-in
profile themes, public author identities, public connection expectations, sanitized
remote URLs, repository-local author fields, hidden branch refs, and worktree
checkout/lock descriptions. An export lists failed or omitted repositories. Sources on
SSH/WSL are read through the existing background transport; importing creates local
repositories on the current machine.

The file excludes passwords/tokens, SSH and signing key values/paths, authentication
configuration folders, generated profile configs/leases, arbitrary Git configuration,
shell/editor/signing programs, working files, stashes and Git objects. HTTPS userinfo,
query strings and fragments are stripped; SSH transport usernames are retained while
passwords/query strings/fragments are removed. Command/helper, HTTP, file and ambiguous
URLs are rejected. Local sources are omitted from remotes and must be selected again on
the importing machine. Inspect the preview before sharing a file; public names and old
machine path labels are intentionally included.

**Import setup…** first validates a preview without changing settings. Adding its public
settings creates new profiles and saved authors; it does not replace or activate the
current profile. Imported account host/username expectations remain pending. Tea also
pins the actual public owner behind its local alias. Select this machine's SSH/signing
key files and authentication folders and verify every required account before explicitly
activating an imported profile. Any imported SSH address requires a fresh explicit key
binding, including profiles that formerly relied on an ambient agent. A local agent may
supply the private key matching that explicitly selected key file; agent-only defaults
are not restored. Missing or mismatched setup fails closed. No account is
signed in or switched automatically.

Select repositories individually, choose an existing local destination parent and a new
plain folder name, and review the resulting repository/worktree paths. Previous paths
are labels only. A current-machine local source can be picked explicitly for offline
restoration. Edit restoration addresses in the preview: URLs that relied on unexported
SSH-config aliases must use the intended literal server hostname instead. Confirm those
SSH addresses and select a supported key before restoration; incomplete bindings stay
pending. Repository clones associated with an imported profile require a matching,
ready profile to be active. A clone/checkout never runs imported hooks, templates,
filters or arbitrary config. A private Git working folder/HOME excludes parent/global
URL rewrites, personal credential helpers and `.netrc`; only captured explicit profile
authentication receives its original local auth-folder paths. HTTPS redirects are
disabled. SSH uses the explicit selected key and ignores unexported SSH configuration.
Anonymous HTTPS remains available without silently using another account. The selected
named branch must exist; detached commits
must be available locally. No replacement branch is chosen. A named branch restores its
current source tip, with a notice when it differs from the export. Local-only commits
need a separate object backup. LFS objects/submodules are not installed automatically.

Destinations must be unused. Linked worktrees go below a separately created sibling
`<repository>-worktrees` directory; existing paths are refused. Partial outcomes retain
successful steps and offer retries while the same app session owns an unchanged clone
and configuration. Changed/missing/replaced owned paths refuse retry. Cancellation
retains completed work; it does not roll back/delete repositories. Failed initial clones
are retained for inspection and require a fresh destination. Quitting loses private
retry receipts, not files: finish the retained clone manually or choose a new unused
destination. Successful clones join their corresponding profiles' recent lists and are
opened only through an explicit action.

Version 1 limits: 64 profiles/repositories/worktrees per repository, 256 saved authors,
32 remotes per repository, 16 push URLs per remote and 2,000 hidden branch refs. Portable
ref/folder names use a conservative ASCII subset; unsupported names or non-UTF-8 metadata
produce an explicit export/import error. Worktree-specific author overrides, custom
themes, hook installers, authentication and machine-specific commands require local
setup. Display settings may be restored independently of profiles.

Azure DevOps and Bitbucket expectations retain only public service/account metadata and
the verified public owner ID. Optional credential-store usernames are public login
metadata; Azure CLI authentication directories, access tokens and credential-helper
passwords are excluded. Check a cloud account before exporting so its public owner can
be pinned. Import keeps those profiles pending until local authentication has been
verified again against that owner. An Azure CLI folder must be selected on the new
machine; no old path is applied. Git transport still requires local SSH/Git credential
setup, and isolated restoration never falls back to a personal ambient login.
