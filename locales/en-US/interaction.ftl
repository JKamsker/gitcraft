interaction-operation-abort-am = Abort am
interaction-operation-abort-cherry-pick = Abort cherry-pick
interaction-operation-abort-merge = Abort merge
interaction-operation-abort-rebase = Abort rebase
interaction-operation-abort-revert = Abort revert
interaction-operation-amend = Amend
interaction-operation-apply-file = Apply { $value }
interaction-operation-apply-patch = Apply patch
interaction-operation-apply-stash = Apply stash
interaction-operation-checkout = Checkout { $value }
interaction-operation-checkout-pull = Checkout and pull { $value }
interaction-operation-cherry-pick = Cherry-pick
interaction-operation-commit = Commit
interaction-operation-continue-am = Continue am
interaction-operation-continue-cherry-pick = Continue cherry-pick
interaction-operation-continue-merge = Continue merge
interaction-operation-continue-rebase = Continue rebase
interaction-operation-continue-revert = Continue revert
interaction-operation-create-branch = Create branch { $value }
interaction-operation-create-tag = Create tag { $value }
interaction-operation-delete-branch = Delete branch { $value }
interaction-operation-delete-tag = Delete tag { $value }
interaction-operation-discard = Discard
interaction-operation-drop-stash = Drop stash
interaction-operation-dvc-pull = DVC: Pull
interaction-operation-fast-forward = Fast-forward { $branch } to { $target }
interaction-operation-fetch = Fetch
interaction-operation-ignore = Ignore { $value }
interaction-operation-mark-resolved = Mark resolved
interaction-operation-merge = Merge
interaction-operation-merge-into = Merge { $branch } into { $target }
interaction-operation-pick = Cherry-pick { $value }
interaction-operation-pop-stash = Pop stash
interaction-operation-preparing = Preparing { $operation }
interaction-operation-profile-author = Use the profile's author
interaction-operation-pull = Pull
interaction-operation-push = Push
interaction-operation-push-after = Push after { $operation }
interaction-operation-push-review = Push for review
interaction-operation-push-to = Push { $branch } to { $remote }
interaction-operation-rebase = Rebase
interaction-operation-rebase-onto = Rebase { $branch } onto { $target }
interaction-operation-redo = Redo { $operation }
interaction-operation-release-tag = Create release tag { $value }
interaction-operation-reload = Reload
interaction-operation-rename-branch = Rename branch { $value }
interaction-operation-revert = Revert
interaction-operation-revert-item = Revert { $value }
interaction-operation-set-identity = Set identity
interaction-operation-skip-hooks = Push, skipping hooks
interaction-operation-stage = Stage
interaction-operation-stage-all = Stage all
interaction-operation-stash = Stash
interaction-operation-stop-tracking = Stop tracking { $value }
interaction-operation-sync = Sync
interaction-operation-track-lfs = Track { $value } in LFS
interaction-operation-undo = Undo { $operation }
interaction-operation-unstage = Unstage
interaction-operation-unstage-all = Unstage all

interaction-release-bump = Commits { $files } on { $branch }: “chore(release): { $tag }”

interaction-release-changelog = { $file } (the notes on top)

interaction-release-forge = the forge

interaction-release-publish = { $pre ->
        [yes] Makes the pre-release { $tag } on { $forge } with the notes
       *[no] Makes the release { $tag } on { $forge } with the notes
    }

interaction-release-push-bump = Pushes the version commit to { $remote }'s { $branch } and the tag to { $remote }, both or neither (git push --atomic)

interaction-release-push-tag = Pushes the tag to { $remote }

interaction-release-step-bump = Move the version

interaction-release-step-publish = Make the release on the forge

interaction-release-step-push = Push

interaction-release-step-tag = Tag

interaction-release-step-upload = Attach the files

interaction-release-tag-object = { $kind ->
        [signed] Makes the signed tag { $tag } at { $commit }, its message the notes (git tag -s { $tag })
       *[annotated] Makes the annotated tag { $tag } at { $commit }, its message the notes (git tag -a { $tag })
    }

interaction-release-tag-version = { $kind ->
        [signed] Makes the signed tag { $tag } at that commit, its message the notes (git tag -s { $tag })
       *[annotated] Makes the annotated tag { $tag } at that commit, its message the notes (git tag -a { $tag })
    }

interaction-release-upload = Attaches { $files } to it

interaction-release-note-tip = Only the tip of the branch checked out can move the version in its files

interaction-release-note-dirty = Commit or stash the changes to the files the version is in first

interaction-release-nothing = nothing to move

interaction-release-no-remote = no remote to push to

interaction-release-unknown-forge = The repository isn't on a forge gitcraft knows

interaction-release-tag-empty = Name the version

interaction-release-tag-taken = { $tag } is a tag already

interaction-release-tag-invalid = { $tag } can't be a tag's name

interaction-release-branch-moved = { $branch } isn't checked out at { $commit } any more: plan the release again

interaction-release-files-changed = { $files } changed since the release was planned: commit or stash that first

interaction-release-version-missing = Couldn't find the version in { $file }

interaction-release-write-failed = Couldn't write { $file }: { $reason }

interaction-recipe-path-empty = empty path
interaction-recipe-path-space = starts or ends with a space
interaction-recipe-path-line = a path is one line
interaction-recipe-path-relative = give a path inside the worktree, relative to its top folder
interaction-recipe-path-outside = a path can't go out of the worktree (..)
interaction-recipe-path-whole = that is the whole worktree
interaction-recipe-path = { $path }: { $reason }
interaction-recipe-variable-reserved = Variable "{ $name }": gitcraft sets that one itself, pick another name
interaction-recipe-variable-name = Variable "{ $name }": names are letters, digits and _, not starting with a digit
interaction-recipe-variable-duplicate = Variable "{ $name }" is set twice
interaction-recipe-variable-line = Variable "{ $name }": a value is one line
interaction-recipe-not-saved = The recipe isn't saved: { $problem }
