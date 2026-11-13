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
worktree-cleanup-archive-failed = Archiving failed, so the worktree was kept: { $error }
worktree-cleanup-restore-other-repository = That archive belongs to another repository
worktree-cleanup-restore-not-top = Archived files are restored into a worktree's top folder only
worktree-cleanup-restore-bad-path = The archived path is not a plain path inside the worktree
worktree-cleanup-restore-blocked = A file or link is in the way, or the place is outside the worktree
worktree-cleanup-path-not-utf8 = The worktree's path is not valid UTF-8
worktree-cleanup-put-back = { $error } The worktree was kept and its untracked files were put back.
worktree-cleanup-put-back-failed = { $error } The worktree was kept; its untracked files are in the archive { $archive }.
