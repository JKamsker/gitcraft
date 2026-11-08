interaction-operation-fetch = Fetch
interaction-operation-pull = Pull
interaction-operation-push = Push
interaction-operation-commit = Commit
interaction-operation-amend = Amend
interaction-operation-stage = Stage
interaction-operation-stage-all = Stage all
interaction-operation-unstage = Unstage
interaction-operation-unstage-all = Unstage all
interaction-operation-discard = Discard
interaction-operation-stash = Stash
interaction-operation-pop-stash = Pop stash
interaction-operation-apply-stash = Apply stash
interaction-operation-drop-stash = Drop stash
interaction-operation-set-identity = Set identity
interaction-operation-profile-author = Use the profile's author
interaction-operation-mark-resolved = Mark resolved
interaction-operation-reload = Reload
interaction-operation-sync = Sync
interaction-operation-push-review = Push for review
interaction-operation-skip-hooks = Push, skipping hooks
interaction-operation-apply-patch = Apply patch
interaction-operation-cherry-pick = Cherry-pick
interaction-operation-revert = Revert
interaction-operation-rebase = Rebase
interaction-operation-merge = Merge
interaction-operation-dvc-pull = DVC: Pull
interaction-operation-abort-merge = Abort merge
interaction-operation-abort-rebase = Abort rebase
interaction-operation-abort-cherry-pick = Abort cherry-pick
interaction-operation-abort-revert = Abort revert
interaction-operation-abort-am = Abort am
interaction-operation-continue-merge = Continue merge
interaction-operation-continue-rebase = Continue rebase
interaction-operation-continue-cherry-pick = Continue cherry-pick
interaction-operation-continue-revert = Continue revert
interaction-operation-continue-am = Continue am
interaction-operation-checkout-pull = Checkout and pull { $value }
interaction-operation-checkout = Checkout { $value }
interaction-operation-create-branch = Create branch { $value }
interaction-operation-delete-branch = Delete branch { $value }
interaction-operation-rename-branch = Rename branch { $value }
interaction-operation-release-tag = Create release tag { $value }
interaction-operation-create-tag = Create tag { $value }
interaction-operation-delete-tag = Delete tag { $value }
interaction-operation-pick = Cherry-pick { $value }
interaction-operation-revert-item = Revert { $value }
interaction-operation-stop-tracking = Stop tracking { $value }
interaction-operation-ignore = Ignore { $value }
interaction-operation-track-lfs = Track { $value } in LFS
interaction-operation-apply-file = Apply { $value }
interaction-operation-undo = Undo { $operation }
interaction-operation-redo = Redo { $operation }
interaction-operation-preparing = Preparing { $operation }
interaction-operation-push-after = Push after { $operation }
interaction-operation-push-to = Push { $branch } to { $remote }
interaction-operation-merge-into = Merge { $branch } into { $target }
interaction-operation-rebase-onto = Rebase { $branch } onto { $target }
interaction-operation-fast-forward = Fast-forward { $branch } to { $target }
interaction-settings-compact-tip = Lanes closer together, for a narrower graph column

interaction-accounts-unnamed = Unnamed profile

interaction-custom-display-failed = { $name } failed (exit status { NUMBER($code) })

interaction-custom-display-finished = { $name } finished

interaction-custom-display-index = Command { NUMBER($index) }

interaction-custom-display-json = { $file } can't be read: { $detail }

interaction-custom-display-large = { $file } is larger than { NUMBER($limit) } KiB, so it isn't read

interaction-custom-display-name = “{ $name }”

interaction-custom-display-problem = { $file }: { $command }: { $problem }

interaction-gerrit-amend-invalid = Invalid Gerrit amendment

interaction-gerrit-amend-message = Missing amend message

interaction-gerrit-ancestor = The target branch is not an ancestor of this commit. Fetch and rebase onto the target before pushing for review.

interaction-gerrit-choose = Choose a remote, a fetched target branch and a valid topic

interaction-gerrit-duplicate = Two commits have the same Change-Id ({ $id }); each change needs its own

interaction-gerrit-empty = There are no commits above the target branch to review

interaction-gerrit-footer = Change-Id must be in the final trailer paragraph

interaction-gerrit-format = Change-Id must be I followed by 40 hexadecimal digits (not all zero)

interaction-gerrit-hook = { $commit }: { $reason }. Install Gerrit's commit-msg hook from your server, then amend this commit to add its Change-Id.

interaction-gerrit-keep = Keep the original Change-Id when amending a Gerrit change

interaction-gerrit-merge = Merge commit { $commit } cannot be pushed with this review flow; rebase the series first

interaction-gerrit-missing = missing Change-Id in the message footer

interaction-gerrit-multiple = more than one Change-Id in the footer

interaction-gerrit-one-line = Change-Id must fit on one line

interaction-gerrit-web = Invalid gitcraft.gerritUrl: use an HTTP(S) URL without credentials, query or fragment. Using the remote's web address; Gerrit review submission remains enabled.

interaction-preferences-bad-variable = “{ $variable }” isn't a variable name: capital letters, digits and _, starting with a letter

interaction-preferences-built-in = ${ $variable } is already set by gitcraft

interaction-preferences-empty-line = Its command line is empty

interaction-preferences-missing-name = It has no name

interaction-preferences-nul = It contains a NUL character

interaction-preferences-reserved = ${ $variable } is used by the shell or git; pick another name

interaction-preferences-twice = ${ $variable } is asked for twice

interaction-recipe-not-saved = The recipe isn't saved: { $problem }

interaction-recipe-path = { $path }: { $reason }

interaction-recipe-path-empty = empty path

interaction-recipe-path-line = a path is one line

interaction-recipe-path-outside = a path can't go out of the worktree (..)

interaction-recipe-path-relative = give a path inside the worktree, relative to its top folder

interaction-recipe-path-space = starts or ends with a space

interaction-recipe-path-whole = that is the whole worktree

interaction-recipe-variable-duplicate = Variable "{ $name }" is set twice

interaction-recipe-variable-line = Variable "{ $name }": a value is one line

interaction-recipe-variable-name = Variable "{ $name }": names are letters, digits and _, not starting with a digit

interaction-recipe-variable-reserved = Variable "{ $name }": gitcraft sets that one itself, pick another name
