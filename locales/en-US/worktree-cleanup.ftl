# Cleaning up the worktrees of merged branches, and the archives of their untracked files (#213).

worktree-cleanup-in-use-this-tab = this tab shows it
worktree-cleanup-in-use-git = Git is working in it (its index is locked)
worktree-cleanup-blocker-primary = the repository's main worktree
worktree-cleanup-blocker-missing = its folder is gone or Git no longer lists it
worktree-cleanup-blocker-bare = a bare repository
worktree-cleanup-blocker-locked = locked: { $reason }
worktree-cleanup-blocker-branch-changed = it has another branch checked out now
worktree-cleanup-blocker-moved = the branch has new commits since it was found merged
worktree-cleanup-blocker-not-merged = no longer found merged
worktree-cleanup-blocker-changed = { $count ->
        [one] { $count } changed tracked file
       *[other] { $count } changed tracked files
    }
worktree-cleanup-blocker-operation = a merge, rebase, cherry-pick, revert or bisect is in progress
worktree-cleanup-blocker-in-use = in use: { $why }
worktree-cleanup-blocker-untracked = { $count ->
        [one] { $count } untracked file (archive it to clean up)
       *[other] { $count } untracked files (archive them to clean up)
    }
worktree-cleanup-blocker-not-archivable = { $count ->
        [one] { $count } untracked file can't be archived (binary, too large, a link or unreadable)
       *[other] { $count } untracked files can't be archived (binary, too large, links or unreadable)
    }
worktree-cleanup-blocker-unpublished = { $count ->
        [one] { $count } commit is neither in the base nor on a remote
       *[other] { $count } commits are neither in the base nor on a remote
    }
worktree-cleanup-blocker-remote = worktrees on another machine are not cleaned up from here
worktree-cleanup-archive-bad-key = Not an archive of this store
worktree-cleanup-archive-quota = The archive would bring archive storage to { $needed }, over the limit of { $quota }. Nothing was removed. Delete old archives or raise the limit in Settings → Git.
worktree-cleanup-archive-no-name = Could not find a free name for the archive
worktree-cleanup-archive-unverified = The archive could not be read back as it was written
worktree-cleanup-archive-changed = { $path } changed while it was being archived
worktree-cleanup-archive-not-text = { $path } is not a text file
worktree-cleanup-archive-inside = The archive location is inside this worktree, which would delete the archive with it. Choose another folder in Settings → Git → Worktree archives.
worktree-cleanup-archive-failed = Archiving failed, so the worktree was kept: { $error }
worktree-cleanup-restore-other-repository = That archive belongs to another repository
worktree-cleanup-restore-not-top = Archived files are restored into a worktree's top folder only
worktree-cleanup-restore-bad-path = The archived path is not a plain path inside the worktree
worktree-cleanup-restore-blocked = A file or link is in the way, or the place is outside the worktree
worktree-cleanup-path-not-utf8 = The worktree's path is not valid UTF-8
worktree-cleanup-put-back = { $error } The worktree was kept and its untracked files were put back.
worktree-cleanup-put-back-failed = { $error } The worktree was kept; its untracked files are in the archive { $archive }.
worktree-cleanup-title = Clean up worktrees of merged branches
worktree-cleanup-intro = Worktrees whose branch is merged. Selected trees, including ignored files and Git metadata, are moved into recovery; branches are kept. No disk space is reclaimed until a separate confirmed purge.
worktree-cleanup-basis = Merged means merged into { $base } (the default branch set for this repository), checked against { $refs }.
worktree-cleanup-checking = Which branches are merged is still being read…
worktree-cleanup-reading = Looking at the worktrees…
worktree-cleanup-none = No worktree has a merged branch checked out.
worktree-cleanup-summary = Retains { $size } · { $changed } changed · { $untracked } untracked ({ $text } text) · { $ignored } ignored ({ $ignored-size })
worktree-cleanup-show-files = Show files
worktree-cleanup-hide-files = Hide files
worktree-cleanup-more-files = …and { $count } more
worktree-cleanup-file-text = archived when archiving is on
worktree-cleanup-file-binary = not archived: binary
worktree-cleanup-file-large = not archived: larger than 1 MiB
worktree-cleanup-file-link = not archived: a link
worktree-cleanup-file-other = not archived: not a plain readable file
worktree-cleanup-archive-option = Archive untracked text files before cleanup
worktree-cleanup-archive-option-tip = Untracked text files go into a verified ZIP with a manifest, in the archive folder (Settings → Git → Worktree archives), before the worktree is removed. Binary, large and linked files are never archived: a worktree with any stays. If archiving fails, nothing is removed.
worktree-cleanup-confirm = { $count ->
        [one] I reviewed the list: remove the { $count } ticked worktree. Its branch is kept.
       *[other] I reviewed the list: remove the { $count } ticked worktrees. Their branches are kept.
    }
worktree-cleanup-remove = { $count ->
        [one] Remove { $count } worktree
       *[other] Remove { $count } worktrees
    }
worktree-cleanup-look-again = Look again
worktree-cleanup-removed = removed, { $freed } given back
worktree-cleanup-removed-archived = { $count ->
        [one] removed, { $freed } given back; { $count } untracked file archived
       *[other] removed, { $freed } given back; { $count } untracked files archived
    }
worktree-cleanup-kept = kept: { $why }
worktree-cleanup-changed = kept: it changed since it was looked at. Look again before removing it.
worktree-cleanup-archives = Archived untracked files
worktree-cleanup-no-archives = No archive of this repository.
worktree-cleanup-archive-usage = Archives use { $used } of { $quota }.
worktree-cleanup-archive-line = { $branch } · { $count ->
        [one] { $count } file
       *[other] { $count } files
    } · { $size } · { $when }{ $restored ->
        [0] {""}
       *[other] {" "}· restored
    }
worktree-cleanup-restore = Restore
worktree-cleanup-restore-tip = Puts the archived files back into the worktree picked. Nothing there is overwritten: a file already there with other content keeps it, and the archived one goes beside it as name.restored. The archive is kept.
worktree-cleanup-restored = Restored { $created } into { $into } ({ $renamed } beside a different file, { $same } already there, { $failed } failed).
worktree-cleanup-delete = Delete
worktree-cleanup-delete-confirm = Delete for good?
worktree-cleanup-archive-deleted = The archive was deleted.
worktree-cleanup-offer = { $count ->
        [one] { $count } worktree has a merged branch checked out.
       *[other] { $count } worktrees have a merged branch checked out.
    }
worktree-cleanup-offer-button = Review worktree cleanup…
worktree-cleanup-settings-intro = When worktrees of merged branches are cleaned up, their untracked text files can be archived first (opt-in, each time). Archives are never deleted to make room.
worktree-cleanup-settings-label-archives = Worktree archives
worktree-cleanup-settings-label-location = Archive location
worktree-cleanup-settings-label-limit = Archive storage limit
worktree-cleanup-settings-label-auto = Restore automatically
worktree-cleanup-settings-label-list = Archives
worktree-cleanup-settings-location-hint = A folder outside your worktrees; empty for the app's data folder
worktree-cleanup-settings-limit-hint = All archives together; archiving more is refused (and the worktree kept) until old ones are deleted
worktree-cleanup-settings-auto-restore = Restore archived files when the branch is checked out again
worktree-cleanup-settings-auto-restore-tip = When a worktree of the branch appears at the commit it was archived from, its files are put back once, never over a file already there.
worktree-cleanup-settings-none = No archives yet.

worktree-cleanup-in-use-tab = open in another tab
worktree-cleanup-in-use-terminals = { $count } terminals open in it
worktree-cleanup-in-use-agent = agent task: { $agent }

worktree-cleanup-recovery-title = Recoverable worktrees
worktree-cleanup-recovery-hint = Full trees and Git metadata stay here until you explicitly purge them. Cleanup reclaims 0 bytes. The ZIP quota does not cover retained trees.
worktree-cleanup-recovery-empty = No recoverable worktrees.
worktree-cleanup-recovery-restore = Restore full worktree
worktree-cleanup-purge = Purge…
worktree-cleanup-purge-confirm = Permanently purge all files?
worktree-cleanup-recovery-done = Recovery action completed.
worktree-cleanup-recovered-at = Full tree retained at { $path }
worktree-cleanup-terminal-unknown = A terminal location is unavailable; close it before cleanup.

# Read-only worktree resource observations (#192)
resources-refresh = Measure / Refresh
resources-stop = Stop
resources-observation = Read-only observation; refresh to inspect current state. Activity means checkpoint/commit observation, never last use.
resources-not-measured = Resources not measured.
resources-branch = Branch
resources-status = Staged / unstaged / conflicted / untracked
resources-local-refs = Publication: local remote-tracking refs only; remote freshness unknown.
resources-external-unknown = Blocked: external processes and ports cannot be exhaustively associated; inactivity unknown.
resources-tracked = Tracked
resources-untracked = Nonignored untracked
resources-build = Disposable build outputs (candidate)
resources-ignored = Other ignored (protected)
resources-unclassified = Unclassified
resources-partial = Partial
resources-unavailable = Unknown / unavailable
resources-platform = Safe filesystem inspection unsupported on this platform.
resources-preview = Preview cleanup
resources-preview-note = Time-bound observations only. Any later execution must inspect again. Logical sizes are not physical recovery estimates (hard links, sparse files, compression). Alternative scopes overlap: do not add their sizes.
resources-retire = Retire clean completed worktree to recovery
resources-logical = Logical bytes
resources-reclaim = Reclaimed now
resources-purge = Potential later purge size
resources-incomplete = Blocked: incomplete measurement
resources-dirty = Blocked: changed or untracked work
resources-active = Blocked: known view, terminal or task
resources-no-build = Blocked: no unambiguous reproducible build root
resources-registration-blocked = Blocked: primary/current/locked/bare/missing worktree
resources-unpublished = Blocked: unpublished commits or insufficient publication evidence
resources-merge-unknown = Blocked: completed merge evidence unavailable
resources-stale = State changed during measurement; refresh
resources-depth = Traversal depth limit reached
resources-cancelled = Measurement cancelled
resources-budget = Entry or elapsed-time budget reached
resources-nested = Nested repository/submodule boundary
resources-link = Symbolic link or special-file boundary
resources-ambiguous-build = Blocked: candidate contains files not recognized as disposable artifacts
resources-remote = SSH/WSL: Git metadata only; sizing, process association and cleanup previews unsupported.
resources-busy = Resource measurement capacity occupied; retry after running jobs exit.
resources-protected-data = Blocked: other ignored or unclassified files may contain user data.
resources-detail-truncated = Unknown path detail truncated after 32 entries; the entire root remains uncertain.
