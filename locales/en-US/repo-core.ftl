repo-core-0006 = Merge { $name } into { $current }
repo-core-0011 = Fast-forward { $current } to { $name }
repo-core-0013 = Rebase { $current } onto { $name }
repo-core-0014 = Cherry-pick { $hash }
repo-core-0016 = Abort { $what }
repo-core-0017 = Stop the { $what } and go back to where it started? Conflicts resolved so far are lost, and can't be brought back.
repo-core-0018 = Abort
repo-core-0020 = DVC: Pull
repo-core-0021 = data download remote
repo-core-0022 = Code moved across files
repo-core-0023 = refactor moved block
repo-core-0024 = File migration map
repo-core-0025 = files
repo-core-0026 = Three-state staging lens
repo-core-0027 = staging
repo-core-0028 = File versions across branches
repo-core-0029 = Fetch
repo-core-0030 = Pull
repo-core-0031 = Push for review
repo-core-0032 = Push
repo-core-0033 = Sync (pull, then push)
repo-core-0034 = remote
repo-core-0035 = Push for review, skipping hooks
repo-core-0036 = Push, skipping hooks
repo-core-0038 = Behaviour timeline…
repo-core-0039 = command output benchmark screenshot
repo-core-0040 = Linked project copies…
repo-core-0041 = machines
repo-core-0042 = Export Git bundle…
repo-core-0043 = offline backup
repo-core-0044 = Inspect local Git bundle…
repo-core-0045 = offline exchange
repo-core-0046 = Stash only what is staged…
repo-core-0047 = stash
repo-core-0048 = Save snapshot (stash, keep the changes)…
repo-core-0049 = Distribute staged fixes…
repo-core-0050 = absorb
repo-core-0051 = Undo
repo-core-0052 = Redo
repo-core-0053 = Timeline and undo tree
repo-core-0054 = Stash changes…
repo-core-0055 = Stage all changes
repo-core-0056 = Commit
repo-core-0057 = Search commits
repo-core-0058 = Reflog
repo-core-0059 = recovery
repo-core-0060 = Find lost commits
repo-core-0061 = Toggle terminal
repo-core-0062 = Go online
repo-core-0063 = network offline
repo-core-0064 = Go offline
repo-core-0065 = network offline metered
repo-core-0066 = Activity: operations and their output
repo-core-0067 = log
repo-core-0068 = New terminal
repo-core-0069 = terminal
repo-core-0070 = Reload
repo-core-0071 = Select stale or merged branches…
repo-core-0072 = Conflicts that can wait: isolated rebase and restack…
repo-core-0073 = Deferred rewrites
repo-core-0074 = Parallel branches in this folder: hunk ownership…
repo-core-0075 = Virtual branches
repo-core-0076 = Inspect state publication receipts…
repo-core-0077 = Working-state transaction recovery
repo-core-0078 = Named changelists: messages, commit and shelve…
repo-core-0079 = Working changes
repo-core-0080 = Push outbox: queue exact commit or send selected entries…
repo-core-0081 = Offline publication
repo-core-0082 = Subtrees: register, update, inspect and publish vendored folders…
repo-core-0083 = Vendored source history
repo-core-0084 = Shared and borrowed objects
repo-core-0085 = Inspect shared Git directories and donors; make a clone independent
repo-core-0086 = Publish submodule dependencies before parent…
repo-core-0087 = submodules publish remote
repo-core-0088 = Pull and push routes…
repo-core-0089 = triangular upstream ahead behind
repo-core-0090 = Publish to several hosts…
repo-core-0091 = publication group retry mirrors
repo-core-0092 = Remote URLs, refspecs and mirror policy…
repo-core-0093 = fetch push mapping prune
repo-core-0094 = Replace refs and temporary history joins…
repo-core-0095 = grafts original effective history
repo-core-0096 = Ref storage and reftable migration…
repo-core-0097 = refs compatibility
repo-core-0098 = How an object is stored…
repo-core-0099 = objects pack delta file version
repo-core-0101 = All refs and namespace history…
repo-core-0102 = refs internals
repo-core-0103 = Repository doctor
repo-core-0104 = doctor
repo-core-0105 = Start agent task…
repo-core-0106 = agent
repo-core-0107 = Bake-off: compare attempts…
repo-core-0108 = Agent turns: save each turn in this worktree
repo-core-0109 = Worktree setup…
repo-core-0110 = What a package will publish
repo-core-0112 = folder
repo-core-0113 = Open in editor
repo-core-0114 = Doctor: { $title }
repo-core-0115 = Open in browser
repo-core-0116 = Check CODEOWNERS
repo-core-0117 = owners
repo-core-0118 = Stop remembering conflict resolutions (rerere)
repo-core-0119 = Remember conflict resolutions (rerere)
repo-core-0120 = conflicts
repo-core-0121 = Remove unused fork remotes ({ $unused })
repo-core-0122 = Update submodules
repo-core-0123 = submodule
repo-core-0124 = Unity: Set-up…
repo-core-0125 = settings
repo-core-0126 = LFS: Fetch
repo-core-0127 = lfs
repo-core-0129 = LFS: Pull
repo-core-0131 = LFS: Locks…
repo-core-0132 = LFS: Storage…
repo-core-0133 = Start work from an issue…
repo-core-0134 = issue
repo-core-0135 = Show CI for HEAD
repo-core-0136 = builds
repo-core-0137 = Finish hotfix…
repo-core-0138 = release
repo-core-0139 = Cut release…
repo-core-0140 = Create branch
repo-core-0141 = Create tag
repo-core-0143 = Download the data referenced by this checkout from its configured DVC remote and materialize it in the working tree? DVC must be installed on the repository machine. Review local data edits first; this operation has no automatic undo.
repo-core-0144 = Pull data
repo-core-0148 = Set identity
repo-core-0154 = Use the profile's author
repo-core-0155 = Ignore { $pattern }
repo-core-0156 = { $pattern } is in .gitignore, but { $path } is still tracked
repo-core-0162 = Added { $pattern } to .gitignore
repo-core-0163 = { $pattern } was already in .gitignore
repo-core-0164 = { $verb ->
                [Stage] { $n ->
                [one] Stage { $n } line of { $path }
               *[other] Stage { $n } lines of { $path }
            }
                [Unstage] { $n ->
                [one] Unstage { $n } line of { $path }
               *[other] Unstage { $n } lines of { $path }
            }
                [Discard] { $n ->
                [one] Discard { $n } line of { $path }
               *[other] Discard { $n } lines of { $path }
            }
               *[Revert] { $n ->
                [one] Revert { $n } line of { $path }
               *[other] Revert { $n } lines of { $path }
            }
            }
repo-core-0166 = Preparing { $label }
repo-core-0167 = Changes reloaded; pick the hunk again before applying it.
repo-core-0169 = { $label }: not done
repo-core-0172 = Notebooks are staged without their outputs from now on (by { $tool }, in this clone): git status may show them changed until they are committed once so
repo-core-0173 = gitcraft no longer strips outputs, but the repository's { $name } filter still does
repo-core-0174 = Notebooks are staged with their outputs again
repo-core-0175 = Could not change how notebooks are staged: { $e }
repo-core-0176 = Mergiraf could not solve the conflicts in { $path }
repo-core-0177 = Mergiraf solved every conflict in { $path }: check the result, then save
repo-core-0178 = Mergiraf solved { $solved } of { $before } conflicts in { $path }; { $left } left to choose
repo-core-0179 = Mark resolved
repo-core-0180 = Saved { $file }, but not marked resolved
repo-core-0182 = { $left }: { $busy } was still running. Try again once it's done.
repo-core-0191 = Could not run { $editor }: { $e }
repo-core-0197 = Working out who wrote { $branch }…
repo-core-0198 = There is no main branch to set { $name } against
repo-core-0204 = agent-written commits
repo-core-0205 = Agent-written
repo-core-0206 = Step through the commits an agent wrote or co-wrote, as they record it (git-ai notes, Agent Trace, an agent's account or Co-authored-by trailer)
repo-core-0207 = Compact lanes
repo-core-0208 = Trunk · release · feature
repo-core-0209 = Git flow
repo-core-0220 = Band layout cancelled
repo-core-0221 = Graph tools…
repo-core-0222 = Walk graph topology
repo-core-0223 = Sparse checkout and downloads…
repo-core-0224 = Clean history in a fresh clone…
repo-core-0225 = Watch the repository grow…
repo-core-0226 = Churn and contributor atlas…
repo-core-0227 = Activity calendar…
repo-core-0228 = Contributor statistics…
repo-core-0230 = Pin branch to band…
repo-core-0231 = Band { $band }
repo-core-0232 = Use preset
repo-core-0233 = Clear all pins
repo-core-0234 = Bands ordered left to right · branch colours remain stable across reloads
repo-core-0235 = Updating branch-model layout…
repo-core-0236 = The visible layout is a preview until the selected branch model finishes rebuilding.
repo-core-0240 = the read stopped unexpectedly
repo-core-0241 = merged into { $base }
repo-core-0242 = its commits are in { $base } (rebased or cherry-picked)
repo-core-0243 = #{ $n } merged it into { $base }
repo-core-0244 = deleted on the remote
repo-core-0245 = { $days ->
            [one] no commit for { $days } day
           *[other] no commit for { $days } days
        }
repo-core-0246 = Could not tell which branches are merged ({ $e }). Only branches deleted on their remote are listed.
repo-core-0250 = Path
repo-core-0251 = Tree
repo-core-0255 = folder { $name }
repo-core-0260 = Inspect local Git bundle (temporary copy to repository machine)
repo-core-0261 = Git bundle
repo-core-0263 = Read bundle refs: { $e }
repo-core-0264 = Inspect bundle: { $e }
repo-core-0265 = Save bundle on this machine (choose a new file)
repo-core-0267 = Export bundle to { $file }
repo-core-0269 = { $label }: done
repo-core-0270 = { $label } failed: { $e }
repo-core-0278 = First commit created
repo-core-0279 = Release tag created
repo-core-0280 = Merge completed
repo-core-0283 = builds: { $tally }
repo-core-0284 = opens the builds
repo-core-0285 = CI: { $state }
repo-core-0286 = CI: { $tally }
repo-core-0287 = { "" }
            … and { $len } more
repo-core-0288 = { "" }
            Click for details
repo-core-0289 = passed
repo-core-0290 = failed
repo-core-0291 = running
repo-core-0292 = skipped
repo-core-0293 = Install git-filter-repo on this machine and reopen this wizard. { $e }
repo-core-0294 = Clean history in a fresh clone
repo-core-0296 = Source (never rewritten): { $self }
repo-core-0297 = Local repositories only. Creates a fresh full mirror with no shared objects, then runs installed git-filter-repo without --force. The new clone and maps remain on disk even on failure/cancel. Closing this tab cancels owned work.
repo-core-0298 = Installed git-filter-repo: { $version }
repo-core-0299 = Output parent folder (a new owned subfolder will be created)
repo-core-0300 = Browse…
repo-core-0301 = Remove literal repository-relative paths, one per line (maximum 32). Include historical names: renames are not followed. Folder paths may end with /.
repo-core-0302 = Literal text replacements (maximum 32); text blobs only. Messages, binary blobs, LFS storage and other clones/forks/caches need separate review.
repo-core-0303 = Exact text to find
repo-core-0304 = Replacement (empty deletes)
repo-core-0305 = Remove
repo-core-0306 = Add literal replacement
repo-core-0307 = Revoke/rotate leaked secrets now. Rewriting history does not revoke them. Replacement rules briefly exist in a new file in the owned folder, then are removed; choose a private output folder, especially on Windows (inherited ACLs apply).
repo-core-0308 = I will inspect the new clone and changed refs before any separate publication
repo-core-0309 = Create fresh clone and rewrite
repo-core-0310 = Checking installed capability…
repo-core-0311 = Cloning, rewriting and checking the owned mirror…
repo-core-0312 = Cancel cleanup
repo-core-0313 = Owned rewritten mirror: { $report }
repo-core-0314 = Copy rewritten repository path
repo-core-0315 = { $count ->
            [one] { $count } changed ref (old → new; ∅ means absent)
           *[other] { $count } changed refs (old → new; ∅ means absent)
        }
repo-core-0316 = Managed filter-repo result
repo-core-0317 = Inspect filter-repo's commit-map/ref-map inside the mirror. Integrity and literal blob checks are bounded checks, not proof that every secret is gone. Residual scans cap at 20000 reachable objects, 4 MiB per blob and 16 MiB total; skipped binary blobs or exceeded limits are unverified. Inspect the intended history and all failed checks yourself.
repo-core-0318 = Separate publishing step — no remote write is performed by this wizard.
repo-core-0319 = Resolve the failed result checks before publishing.
repo-core-0320 = 1. Revoke or rotate exposed secrets at their issuer; audit access. Rewriting cannot revoke a credential.
repo-core-0321 = 2. Inspect the new mirror and coordinate the rewrite with collaborators. Retain an offline backup of the original; treat it as sensitive.
repo-core-0322 = 3. Deliberately add a publishing remote to the NEW mirror. Compare each destination ref against its current remote value and use explicit per-ref force-with-lease publication through your existing guarded publisher or terminal. Do not use an unreviewed mirror/force push.
repo-core-0323 = 4. Ask collaborators to re-clone. Review forge PR refs, forks, caches, release assets and LFS storage separately. The wizard's origin has been removed to prevent accidental publication back to the source.
repo-core-0324 = Review separate publishing and revocation steps
repo-core-0325 = What usually comes with this?
repo-core-0327 = Reading local change coupling…
repo-core-0328 = History hints unavailable: { $error }
repo-core-0329 = { $count } qualifying commits; { $skipped } bulk/generated/unsupported commits excluded{ $else }
repo-core-0330 = ; sample is incomplete
repo-core-0331 = { $companion } usually changes with { $source } ({ $count }/{ $source-changes } source changes, { $source-changes2 }%) — it has no working or staged change
repo-core-0332 = Dismiss
repo-core-0333 = { $count ->
            [one] Evidence: { $count } shared commit
           *[other] Evidence: { $count } shared commits
        }
repo-core-0334 = Open this actual past commit to inspect both file changes
repo-core-0335 = No qualifying undismissed companion-file hints for the staged paths.
repo-core-0336 = Restore dismissed hints
repo-core-0337 = Refresh history hints
repo-core-0339 = Checkout { $target }
repo-core-0340 = Checkout and pull { $branch }
repo-core-0341 = Checkout { $local }
repo-core-0345 = Gerrit review
repo-core-0346 = its configured push route
repo-core-0348 = Stop tracking { $upstream }
repo-core-0351 = Resolve the conflicted files first
repo-core-0352 = Continue { $to-lowercase }
repo-core-0355 = Write a commit summary first
repo-core-0358 = Amend
repo-core-0360 = Create { $noun }
repo-core-0369 = Can't check out "{ $name }": not a usable name
repo-core-0370 = Cherry-pick { $short }
repo-core-0371 = Revert { $short }
repo-core-0373 = Soft
repo-core-0374 = Keeps every change, staged
repo-core-0375 = Keeps every change, staged.
repo-core-0376 = Mixed
repo-core-0377 = Keeps every change, unstaged
repo-core-0378 = Keeps every change, unstaged.
repo-core-0380 = Keep
repo-core-0381 = Keeps local changes; stops if changed files would be touched
repo-core-0382 = Moves the branch while preserving uncommitted changes. Stops without moving the branch if a file changed locally must also change for this reset.
repo-core-0383 = Hard
repo-core-0384 = Throws away uncommitted changes and the commits after this one
repo-core-0385 = Throws away uncommitted changes and the commits after this one. Uncommitted work can't be recovered.
repo-core-0387 = { $label } reset
repo-core-0388 = Move { $target } to { $short } ({ $summary })? { $explain }
repo-core-0389 = Reset { $to-lowercase }
repo-core-0392 = Finish or close the current editor before following a backlink
repo-core-0393 = This commit has no file changes to show
repo-core-0402 = { $name } can't be read: is the operation still in progress?
repo-core-0403 = { $head } has no parent (a first commit): there is no history to look into
repo-core-0404 = No other branch to compare with
repo-core-0416 = { $hash } can't be read
repo-core-0425 = Finding where it started needs a newer git: 2.38 for a merge, 2.40 for a rebase, cherry-pick or revert (merge-tree --write-tree)
repo-core-0426 = The test merge failed: { $e }
repo-core-0432 = { "" }
            … { $MAX-PATCH-LINES } more lines not shown
repo-core-0433 = the commits changed meanwhile
repo-core-0436 = Reading each side's commits…
repo-core-0437 = Try again
repo-core-0438 = Testing merges…
repo-core-0439 = 1 test merge so far…
repo-core-0440 = { $n ->
            [one] { $n } test merge so far…
           *[other] { $n } test merges so far…
        }
repo-core-0441 = Show or hide each side's history
repo-core-0442 = HOW THIS CONFLICT CAME ABOUT
repo-core-0443 = Cancel
repo-core-0444 = Stop looking
repo-core-0445 = Find where it started
repo-core-0446 = Find the first pair of commits, one from each side, whose merge already conflicts in this file. Each test merge is done with git merge-tree: your files and the index are left as they are.
repo-core-0447 = Each side's commits are read first
repo-core-0448 = 1 commit
repo-core-0449 = { $n ->
            [one] { $n } commit
           *[other] { $n } commits
        }
repo-core-0450 = { $name } · { $count } on this file
repo-core-0451 = No commit of this side touched the file since they diverged.
repo-core-0454 = ⚑ where the conflict started
repo-core-0455 = Click to pick it: what it changed, and a test merge
repo-core-0456 = What { $short } changed
repo-core-0457 = Its change to this file
repo-core-0458 = Test these two
repo-core-0459 = Merge them with git merge-tree: does this file conflict?
repo-core-0460 = { $short } and { $short2 } conflict in this file
repo-core-0461 = { $short } and { $short2 } merge without a conflict here
repo-core-0463 = The commits changed meanwhile: close the file and open it again
repo-core-0464 = ⚑ Where it started: { $a } ({ $names }) and { $b } ({ $names2 }). Their merge already conflicts in this file
repo-core-0465 = , while the commits just before each don't (found by halving each side, as git bisect does: a conflict that came and went earlier can be missed).
repo-core-0466 = . The history isn't a straight line here, so an earlier pair may conflict too.
repo-core-0467 = { " " }It may have started earlier: only the latest { $MAX-COMMITS } commits of a side are read.
repo-core-0468 = Only one side has commits on this file since they diverged, so no pair of them conflicts: the conflict may come from a rename (not followed here), a deletion or the file's mode.
repo-core-0469 = Merged on their own, the two sides' latest commits don't conflict in this file: the conflict may come from a rename, or from how the operation merges (its options, what it replayed before).
repo-core-0470 = ← Back to the commits
repo-core-0471 = What { $short } changed in this file
repo-core-0472 = (no change to this file)
repo-core-0476 = The notes were not kept: the text around the conflicts was edited by hand, so which part resolves which conflict can't be told
repo-core-0479 = Test “{ $command }” passes
repo-core-0480 = no output
repo-core-0481 = Test “{ $command }” fails: { $last }
repo-core-0482 = Test “{ $command }” could not run: { $e }
repo-core-0483 = Kept the decision
repo-core-0484 = { $n ->
            [one] Kept { $n } decision
           *[other] Kept { $n } decisions
        }
repo-core-0485 = { $others-conflicted ->
                [one]  ({ $others-conflicted } other file still had conflicts )
               *[other]  ({ $others-conflicted } other files still had conflicts )
            }
repo-core-0486 = { $Fetching } conflict decisions with { $remote }…
repo-core-0487 = Sending
repo-core-0488 = Fetching
repo-core-0489 = Applied a decision made before: check it, then save
repo-core-0490 = { $n ->
            [one] Applied { $n } decision made before: check them, then save
           *[other] Applied { $n } decisions made before: check them, then save
        }
repo-core-0491 = Kept the decisions; running their tests…
repo-core-0492 = Send conflict decisions to { $remote }
repo-core-0493 = Get conflict decisions from { $remote }
repo-core-0500 = Could not read the conflict decisions: { $trim }
repo-core-0502 = { $key } can't name a conflict decision
repo-core-0503 = Could not write the decision down: { $e }
repo-core-0507 = Could not store the decision: { $trim }
repo-core-0510 = Could not store the decision: { $e }
repo-core-0515 = Could not get the conflict decisions from { $remote }: { $e }
repo-core-0517 = No newer conflict decisions on { $remote }
repo-core-0518 = Took 1 newer conflict decision from { $remote }
repo-core-0519 = { $n ->
            [one] Took { $n } newer conflict decision from { $remote }
           *[other] Took { $n } newer conflict decisions from { $remote }
        }
repo-core-0520 = { $got }; no conflict decisions to send
repo-core-0522 = { $got }, but could not send them to { $remote }: { $e }
repo-core-0523 = { $count ->
                [one] { $got }; sent { $count } conflict decision to { $remote }
               *[other] { $got }; sent { $count } conflict decisions to { $remote }
            }
repo-core-0524 = Test “{ $test }” passed { $at }
repo-core-0525 = Test “{ $test }” failed { $at }
repo-core-0526 = Test “{ $test }”: not run yet
repo-core-0528 = Decided before, but the code around it changed since
repo-core-0529 = Resolved as you decided before
repo-core-0530 = Decided before
repo-core-0531 = Output
repo-core-0533 = Not applied by itself: check that it still fits before you use it.
repo-core-0534 = Use it anyway
repo-core-0535 = Use this decision
repo-core-0536 = Resolve this conflict with the text decided before
repo-core-0537 = The result is edited by hand: Back to choices first
repo-core-0538 = Keep its note and test
repo-core-0539 = Take its reason and test as yours for this resolution: kept when you save, and the test run then if you ask
repo-core-0540 = What changed around it
repo-core-0541 = { "  " }⋯ the conflict ⋯
            { "" }
repo-core-0543 = Then
repo-core-0544 = Now
repo-core-0546 = Note why…
repo-core-0547 = Edit the note…
repo-core-0548 = Keep a note on why it is resolved so, and a command that tests it: when the same conflict comes back, it is resolved the same way, with the note shown.
repo-core-0549 = Kept when saved: the test “{ $test }”
repo-core-0550 = Kept when saved: “{ $note }”
repo-core-0552 = Why
repo-core-0553 = keep the new validation, keep the old wire format
repo-core-0554 = Test
repo-core-0555 = a command that checks it, e.g. cargo test wire_format
repo-core-0556 = Kept in git with the resolution when you save.
repo-core-0557 = Done
repo-core-0558 = Test after saving
repo-core-0559 = Runs the tests you wrote, in the repository's folder, once the file is saved:
            { $n }
            How each went is kept with its decision.
repo-core-0569 = no free name found for { $path }: { $tag } and 99 more are taken
repo-core-0571 = could not check { $file }: { $e }
repo-core-0573 = { $who } deleted { $path }, so there is only one version to keep. To keep it, use Mark resolved; to drop it, delete the file, then Mark resolved
repo-core-0574 = { $who }'s { $path } is a symbolic link, not a file
repo-core-0575 = { $who }'s { $path } is a submodule, not a file
repo-core-0576 = { $path } is no longer in conflict
repo-core-0577 = Upstream
repo-core-0578 = Your commit
repo-core-0579 = Ours
repo-core-0580 = Theirs
repo-core-0582 = Could not read the version of { $who }: { $e }
repo-core-0583 = Could not read the version of { $origin }: { $e }
repo-core-0584 = Could not write { $target }: { $e }
repo-core-0585 = a file appeared there after the preview, so nothing was overwritten and nothing is left written. Try again to pick free names
repo-core-0589 = { $path } is no longer in the conflict Keep both was opened for, so nothing was staged or removed. { $target } and { $target2 } are still there to check.
repo-core-0590 = Keep both versions of { $path }
repo-core-0593 = Reading both versions…
repo-core-0594 = Both versions could not be kept
repo-core-0595 = Close
repo-core-0596 = Writing both files…
repo-core-0597 = Write both files
repo-core-0598 = Wrote { $target } and { $target2 }.{ $them } Nothing is staged yet.
repo-core-0599 = { " " }A .gitignore hides them from the changes list: check them in the folder.
repo-core-0600 = { " " }They are in the changes list as new files: click them there to check them.
repo-core-0601 = Staging them also removes { $path } and marks its conflict resolved.
repo-core-0602 = Not now leaves them as they are: to finish later, stage both files and delete { $path }.
repo-core-0603 = Stage both
repo-core-0604 = Not now
repo-core-0605 = Neither version is thrown away: each is written as a file of its own, beside { $path }. Nothing is staged before you have checked them.
repo-core-0606 = Anything that refers to { $name } (code, stylesheets, scenes, docs) may need updating to one of the new names.
repo-core-0607 = A .gitignore names this file: it won't show in the changes list, and Stage both adds it anyway
repo-core-0608 = { $version } already there, so not that name
repo-core-0609 = { $k ->
                [one] { $k } line
               *[other] { $k } lines
            }
repo-core-0610 = unchanged
repo-core-0611 = { $r ->
                [one] deleted all { $r } line
               *[other] deleted all { $r } lines
            }
repo-core-0612 = { $r ->
                [one] deleted { $r } line
               *[other] deleted { $r } lines
            }
repo-core-0613 = { $a ->
                [one] inserted { $a } line
               *[other] inserted { $a } lines
            }
repo-core-0614 = { $r ->
                [one] { $a ->
                [one] replaced all { $r } line with { $a } line
               *[other] replaced all { $r } line with { $a } lines
            }
               *[other] { $a ->
                [one] replaced all { $r } lines with { $a } line
               *[other] replaced all { $r } lines with { $a } lines
            }
            }
repo-core-0615 = { $a ->
                [one] { $r ->
                [one] inserted { $a } line, deleted { $r } line
               *[other] inserted { $a } line, deleted { $r } lines
            }
               *[other] { $r ->
                [one] inserted { $a } lines, deleted { $r } line
               *[other] inserted { $a } lines, deleted { $r } lines
            }
            }
repo-core-0616 = this conflict doesn't match the one rebuilt from the index (the file may have been edited since, or merged with options or a merge driver that can't be repeated here)
repo-core-0617 = could not read the index: { $e }
repo-core-0618 = the index no longer has this file in conflict
repo-core-0619 = Hide the original
repo-core-0620 = Show the text both sides started from, and what each changed
repo-core-0621 = Original
repo-core-0622 = reading the base…
repo-core-0623 = none: both sides added this file
repo-core-0624 = not available
repo-core-0625 = what each side changed
repo-core-0626 = There is no original: both sides added this file, so neither changed an earlier version.
repo-core-0627 = The original is not known: { $why }.
repo-core-0628 = The original was empty here: both sides inserted text.
repo-core-0629 = { $name } against the original: { $lines }
repo-core-0631 = … { $MAX-SHOWN } more lines
repo-core-0635 = { $name } can't be read: { $e }
repo-core-0669 = Showing what git rerere reused needs git 2.43 or newer
repo-core-0670 = Could not rebuild the conflicts of { $path }: { $e }
repo-core-0671 = Remembered resolutions: { $e }
repo-core-0673 = Could not turn on remembered resolutions
repo-core-0674 = Could not turn off remembered resolutions
repo-core-0675 = Could not leave reused resolutions unstaged
repo-core-0677 = Conflict resolutions are remembered here: how you resolve a conflict is recorded when you commit or continue, and reused when it comes back
repo-core-0678 = Conflict resolutions are remembered here, but git stages what it reuses by itself
repo-core-0679 = Conflict resolutions are no longer remembered here
repo-core-0680 = Check it: nothing is staged until you click Save and mark resolved.
repo-core-0681 = Resolved as you did before (git rerere). Each conflict below shows the text reused for it. { $check }
repo-core-0682 = Resolved as you did before (git rerere), but the result can't be matched conflict by conflict, so it shows the whole file as it is now. { $check }
repo-core-0683 = This file has no conflict markers left, but git still lists it as conflicted: a resolution was reused (git rerere), or it was edited elsewhere. Each conflict below shows the text now in its place.
repo-core-0684 = This file has no conflict markers left, but git still lists it as conflicted: a resolution was reused (git rerere), or it was edited elsewhere. The result shows the whole file as it is now.
repo-core-0685 = What is in the file now is lost, saved or not.
repo-core-0686 = Forget this resolution
repo-core-0687 = git forgets how these conflicts were resolved before and puts the conflict markers back in the file
repo-core-0688 = Forget this resolution?
repo-core-0689 = git forgets how the conflicts of { $path } were resolved before (git rerere forget), and puts their conflict markers back. { $lost }
repo-core-0690 = Forget it
repo-core-0691 = Bring the conflicts back
repo-core-0692 = Puts the conflict markers back in the file (git checkout -m)
repo-core-0693 = Bring the conflicts back?
repo-core-0694 = { $path } gets its conflict markers back (git checkout -m). { $lost }
repo-core-0695 = Bring them back
repo-core-0696 = Reused: the text now in place of this conflict
repo-core-0697 = (nothing)
repo-core-0699 = Remember resolutions
repo-core-0700 = git rerere, for this repository: how you resolve a conflict is recorded when you commit or continue, and reused when the same conflict comes back. A reused resolution shows here, and isn't staged until you save it. Turning it on also sets rerere.autoUpdate to false here.
repo-core-0701 = Leave them unstaged
repo-core-0702 = Sets rerere.autoUpdate to false for this repository
repo-core-0703 = git stages reused resolutions by itself here
repo-core-0704 = rerere.autoUpdate is on: a file resolved as before is staged at once, so it never shows here to be checked
repo-core-0705 = { $unresolved } unresolved conflict(s)
repo-core-0706 = Local
repo-core-0707 = Remote
repo-core-0709 = { $total } of { $total2 } resolved
repo-core-0711 = Close (Esc)
repo-core-0712 = Send to agent
repo-core-0713 = Types the conflicts, each with its three sides, into the agent's terminal. The agent then edits the file itself: close this editor then, and don't save over its work
repo-core-0714 = the conflicts
repo-core-0715 = Save and mark resolved
repo-core-0716 = Resolve every conflict first
repo-core-0717 = Solve with UnityYAMLMerge
repo-core-0718 = Let Unity's Smart Merge merge this Unity file by its structure. Nothing is saved before you check the result.
repo-core-0719 = Wait for the running operation
repo-core-0720 = UnityYAMLMerge wasn't found: choose it in Unity set-up… (Ctrl+P)
repo-core-0723 = Solve with Mergiraf
repo-core-0724 = Let Mergiraf solve the conflicts it can from the syntax of the code. Nothing is saved before you check the result.
repo-core-0725 = Use theirs
repo-core-0726 = Take their version of the whole file
repo-core-0727 = Use mine
repo-core-0728 = Keep your version of the whole file
repo-core-0729 = { $label } for { $path }
repo-core-0730 = Resolve in merge tool
repo-core-0731 = Resolve this file in the merge tool of Settings → Git → External tools instead; it is marked resolved when the tool reports success. Choices made here and not saved are discarded.
repo-core-0733 = RESULT
repo-core-0734 = the whole file, as it is
repo-core-0735 = It can't be matched conflict by conflict: "Back to choices" drops it and brings the conflict markers back
repo-core-0736 = edited by hand
repo-core-0737 = Choices no longer change the result: "Back to choices" applies them, and drops the edits
repo-core-0738 = Back to choices
repo-core-0741 = Conflict { $k } of { $total }
repo-core-0742 = Reused
repo-core-0743 = Both (ours first)
repo-core-0744 = Both (theirs first)
repo-core-0745 = The text git put in place of this conflict, as you resolved it before
repo-core-0747 = the conflict check stopped
repo-core-0748 = internal error
repo-core-0749 = Not checked in a partial clone: git would download the files it lacks
repo-core-0751 = both sides changed it
repo-core-0752 = binary file
repo-core-0754 = changed on one side, deleted on the other
repo-core-0756 = renamed on one side, deleted on the other
repo-core-0758 = renamed differently on each side
repo-core-0761 = a file on one side, a folder on the other
repo-core-0763 = added on both sides
repo-core-0764 = not a text conflict
repo-core-0765 = checking…
repo-core-0766 = check failed
repo-core-0767 = already applied
repo-core-0768 = already merged
repo-core-0769 = applies cleanly
repo-core-0770 = conflicts in { $len }
repo-core-0771 = may conflict
repo-core-0772 = 1 file
repo-core-0773 = { $n ->
            [one] { $n } file
           *[other] { $n } files
        }
repo-core-0774 = Checking for conflicts; nothing in the repository is changed
repo-core-0775 = Couldn't check for conflicts: { $e }
repo-core-0776 = { "" }
            { "" }
            Uncommitted changes aren't part of the check; git may refuse to start if they touch these files
repo-core-0777 = Its changes are already on the current branch: git would stop with an empty cherry-pick
repo-core-0778 = Already merged: there is nothing to bring in
repo-core-0779 = The commit applies
repo-core-0780 = { $commits ->
        [one] { $commits } commit applies
       *[other] All { $commits } commits apply
    }
repo-core-0781 = { $all } without conflicts
repo-core-0782 = No conflicts
repo-core-0783 = No conflicts found, but this may conflict: { $why }
repo-core-0784 = Commit { $index } of { $commits }, { $hash } “{ $summary }”, conflicts in { $len }:
repo-core-0785 = Conflicts in { $len }:
repo-core-0786 = { "" }
            { "" }
            The commits after it aren't checked: they depend on how this is solved.
repo-core-0787 = { "" }
            { "" }
            Mergiraf is installed but couldn't list the file types it knows: { $e }
repo-core-0788 = { "" }
            { "" }
            Mergiraf can solve many text conflicts: Settings → Git → Mergiraf installs it.
repo-core-0789 = Mergiraf can likely solve it
repo-core-0790 = a file type Mergiraf doesn't know
repo-core-0791 = { $kinds }: solve by hand
repo-core-0792 = text conflict
repo-core-0794 = { "" }
            …and { $LISTED } more
repo-core-0796 = This commit is already checked out
repo-core-0797 = Another operation is running
repo-core-0798 = Conflicts with
repo-core-0799 = May conflict with
repo-core-0800 = its upstream
repo-core-0801 = the default branch; it has no upstream
repo-core-0802 = Rebasing { $current } onto { $target } ({ $why }), as Pull does
repo-core-0803 = Merging { $target } ({ $why }) into { $current }
repo-core-0806 = View all files
repo-core-0807 = List every file of the commit, not only those it changed
repo-core-0811 = Changes
repo-core-0812 = Earlier versions…
repo-core-0813 = Opt in to working-tree drafts and recover selected hunks from earlier working or staged versions
repo-core-0817 = { $label } in progress (rebase)
repo-core-0818 = { $label } in progress
repo-core-0819 = Restack paused at { $p } ({ $done } of { $count })
repo-core-0820 = Abort restack
repo-core-0821 = Every branch back where it was before the restack
repo-core-0822 = All conflicts resolved: Continue restack to go on.
repo-core-0823 = Fix conflicted files, mark resolved, then Continue restack.
repo-core-0824 = No conflicts: Continue rebase goes on. At a commit it stopped at to edit, what is staged goes into that commit first.
repo-core-0825 = All conflicts resolved: commit to finish.
repo-core-0826 = Fix the conflicted files, mark them resolved, then commit.
repo-core-0827 = CONFLICTED
repo-core-0829 = Mark resolved (stage)
repo-core-0831 = UNSTAGED
repo-core-0832 = Stage all
repo-core-0834 = Discard changes…
repo-core-0836 = Stage
repo-core-0838 = Nothing to stage
repo-core-0839 = STAGED
repo-core-0840 = Unstage all
repo-core-0841 = Put each staged change into the earlier commit of the branch that last changed those lines
repo-core-0842 = HEAD is detached: check out a branch first
repo-core-0844 = Unstage
repo-core-0846 = Stage files to choose what goes in the commit
repo-core-0851 = Delete new file
repo-core-0852 = Delete “{ $path }”? It isn't tracked by git, so this can't be undone.
repo-core-0853 = Delete file
repo-core-0855 = Deleted
repo-core-0856 = Discard changes
repo-core-0857 = Discard the unstaged changes to “{ $path }”? This can't be undone.
repo-core-0858 = Discard
repo-core-0860 = Unstaged changes lost
repo-core-0862 = Stash
repo-core-0863 = Apply, pop or drop
repo-core-0864 = Prompt & session
repo-core-0865 = Select a commit
repo-core-0868 = vs. first parent
repo-core-0871 = Pop
repo-core-0872 = Apply
repo-core-0874 = Double-click to edit the message
repo-core-0875 = Merges came after this commit: editing its message would flatten them
repo-core-0877 = Committed by { $committer } · { $commit-date }
repo-core-0880 = Copy full hash
repo-core-0881 = Parents
repo-core-0882 = Parent
repo-core-0883 = Builds
repo-core-0884 = Show
repo-core-0886 = Summary
repo-core-0887 = Description (optional)
repo-core-0888 = Save message
repo-core-0889 = Ctrl+Enter saves · Esc cancels
repo-core-0892 = CONTINUE { $to-uppercase }
repo-core-0893 = COMMIT
repo-core-0894 = Amend last commit
repo-core-0895 = Rewrite the last commit with the staged changes and this message
repo-core-0896 = This commit is already pushed: amending rewrites shared history.
repo-core-0897 = The commit keeps its message: reword it once this is done.
repo-core-0898 = { $len ->
            [one] Summary is { $len } character; 50–72 reads best
           *[other] Summary is { $len } characters; 50–72 reads best
        }
repo-core-0899 = Skip hooks
repo-core-0900 = Commit once without the pre-commit and commit-msg hooks (git commit --no-verify); it unticks once the commit runs
repo-core-0901 = Continue restack
repo-core-0902 = { $staged ->
                [one] Commit { $staged } file
               *[other] Commit { $staged } files
            }
repo-core-0903 = Stage all and commit
repo-core-0904 = Edit the message first: it is still the template
repo-core-0905 = this repository
repo-core-0906 = profile { $title }
repo-core-0907 = global ({ $host })
repo-core-0908 = global
repo-core-0909 = No identity set (git config user.name)
repo-core-0911 = { " " }· name: { $name-from }, email: { $email-from }
repo-core-0912 = as { $name } <{ $email }>{ $source }
repo-core-0913 = Set in this repository's own config, which wins over that machine's global one
repo-core-0914 = Set in this repository's own config, which wins over the profile's author
repo-core-0915 = Profile { $title }
repo-core-0917 = Commit as…
repo-core-0918 = Add identities or profile authors in Settings
repo-core-0920 = Sets it in this repository's own config
repo-core-0921 = Takes user.name and user.email out of this repository's own config, so that the profile's (or your global config's) apply
repo-core-0922 = Compare
repo-core-0923 = No differences
repo-core-0926 = the working tree
repo-core-0929 = merge base{ " " }
repo-core-0930 = From
repo-core-0931 = To
repo-core-0932 = { $n ->
                [one] { $n } commit
               *[other] { $n } commits
            }
repo-core-0933 = Same history
repo-core-0934 = { $other } is { $a } ahead
repo-core-0935 = { $other } is { $b } behind
repo-core-0936 = { $other } is { $a } ahead, { $b } behind
repo-core-0937 = Only what { $target-name } changed since they diverged
repo-core-0938 = Diff from the merge base, as a pull request shows it, instead of every difference between the two
repo-core-0939 = Swap
repo-core-0942 = Add co-author
repo-core-0943 = Name or email; Enter adds the first
repo-core-0944 = No other author matches
repo-core-0945 = { $commits ->
                [one] { $commits } commit
               *[other] { $commits } commits
            }
repo-core-0946 = The message changed while the co-author was added: add them again
repo-core-0950 = ↑ ↓ to choose, Tab to complete, Esc to close
repo-core-0957 = The pattern is empty
repo-core-0958 = The pattern is longer than { $MAX-PATTERN } characters
repo-core-0959 = Not a valid pattern: { $s }
repo-core-0960 = The pattern is too complex
repo-core-0961 = Not a valid pattern: { $other }
repo-core-0962 = The URL must start with https:// or http://
repo-core-0965 = Issues
repo-core-0966 = Message options
repo-core-0967 = Type / scope picker
repo-core-0968 = Prefill from prepare-commit-msg
repo-core-0969 = Runs the repository's prepare-commit-msg hook to start an empty message. git runs the hook again when committing: untick this if yours adds its text twice.
repo-core-0970 = Issue links
repo-core-0971 = #12 links to { $replace }
repo-core-0972 = #12 links to the forge's issue when the repository is hosted on one
repo-core-0973 = Remove this rule
repo-core-0974 = Pattern, e.g. \bJIRA-\d+\b
repo-core-0975 = URL, e.g. https://jira.example.com/browse/$0
repo-core-0976 = Add
repo-core-0977 = $0 is what matched, $1, $2… (or ${ "{" }1{ "}" }) its groups
repo-core-0978 = That rule is already there
repo-core-0979 = At most { $MAX-RULES } rules; remove one first
repo-core-0981 = Type and scope (Conventional Commits)
repo-core-0982 = Type
repo-core-0984 = Scope
repo-core-0985 = optional
repo-core-0986 = Used before
repo-core-0987 = Breaking change (!)
repo-core-0988 = Marks the commit as one that breaks compatibility
repo-core-0989 = Remove the prefix
repo-core-0992 = A new feature
repo-core-0994 = A bug fix
repo-core-0996 = Documentation only
repo-core-0998 = Formatting, no change in behaviour
repo-core-1000 = Neither a fix nor a feature
repo-core-1002 = Faster or leaner
repo-core-1004 = Tests added or corrected
repo-core-1006 = The build system or dependencies
repo-core-1008 = The CI configuration
repo-core-1010 = Anything else that isn't code
repo-core-1011 = Reverts an earlier commit
repo-core-1012 = New feature
repo-core-1013 = Bug fix
repo-core-1014 = Critical hotfix
repo-core-1015 = Documentation
repo-core-1016 = Structure or format of the code
repo-core-1017 = Refactor
repo-core-1018 = Performance
repo-core-1019 = Remove code or files
repo-core-1020 = Tests
repo-core-1021 = Configuration
repo-core-1022 = CI build system
repo-core-1023 = Fix the CI build
repo-core-1024 = Upgrade dependencies
repo-core-1025 = Downgrade dependencies
repo-core-1026 = Security or privacy
repo-core-1027 = UI and styles
repo-core-1028 = Translations
repo-core-1029 = Work in progress
repo-core-1030 = Types
repo-core-1031 = Deprecate code
repo-core-1032 = Revert changes
repo-core-1033 = Merge branches
repo-core-1034 = Compiled files or packages
repo-core-1035 = Deploy
repo-core-1036 = Simple fix for a non-critical issue
repo-core-1041 = Off
repo-core-1042 = Conventional Commits
repo-core-1044 = Give the template a name
repo-core-1045 = Write the message to save as a template first
repo-core-1046 = A template can be at most { $MAX-TEMPLATE-LEN } characters
repo-core-1047 = There are already { $MAX-TEMPLATES } templates here; delete one first
repo-core-1049 = Templates and prepare-commit-msg
repo-core-1050 = Start from
repo-core-1051 = Replaces the message with the file commit.template names (Undo brings it back)
repo-core-1053 = This repository sets no commit.template
repo-core-1054 = Run prepare-commit-msg
repo-core-1055 = Runs the repository's prepare-commit-msg hook on the message, as git does before it opens its editor
repo-core-1056 = This repository
repo-core-1057 = All repositories
repo-core-1058 = Replaces the message (Undo brings it back) with:
            { "" }
            { $text }
repo-core-1059 = Delete the template { $name }
repo-core-1060 = Save this message as a template
repo-core-1061 = Name
repo-core-1062 = Save
repo-core-1063 = Write a message first
repo-core-1064 = For all repositories
repo-core-1065 = Unticked, it is offered in this repository only
repo-core-1066 = Could not be searched
repo-core-1067 = Backlinks
repo-core-1068 = Search again
repo-core-1069 = Searching later commits for its hash…
repo-core-1070 = no later commit mentions it
repo-core-1071 = { $explicit ->
                [one] { $explicit } link
               *[other] { $explicit } links
            }
repo-core-1072 = { $bare ->
                [one] { $bare } mention
               *[other] { $bare } mentions
            }
repo-core-1073 = { $text } · only the { $BACKLINK-CAP } most recent
repo-core-1074 = No later commit mentions its hash: no revert, cherry-pick, Fixes: trailer or plain mention.
repo-core-1075 = Links
repo-core-1076 = Mentions
repo-core-1077 = Only the { $BACKLINK-CAP } most recent are listed.
repo-core-1079 = Open changes in { $hash }: { $subject }
repo-core-1080 = { $hash } · { $author } · { $date }
            { "" }
            { $text }
            { "" }
            Click to open this commit’s changes and diff
repo-core-1081 = Could not be read
repo-core-1082 = Contained in
repo-core-1083 = Read again
repo-core-1084 = Looking for the branches and tags that have it…
repo-core-1087 = first tag { $tag }
repo-core-1088 = no { $many }
repo-core-1089 = 1 { $one }
repo-core-1090 = { $n } { $many }
repo-core-1091 = First tag with it
repo-core-1092 = git describe --contains: { $describe }
repo-core-1093 = No tag contains it yet.
repo-core-1094 = Branches
repo-core-1095 = Remote branches
repo-core-1096 = Tags
repo-core-1097 = No branch has it, only tags.
repo-core-1098 = No branch or tag has it. Git may delete it when it cleans up; create a branch to keep it.
repo-core-1099 = and { $hidden } more
repo-core-1100 = { $Push ->
                [Fetch] Fetch notes “{ $namespace }” from { $remote }
               *[Push] Push notes “{ $namespace }” to { $remote }
            }
repo-core-1101 = from
repo-core-1102 = to
repo-core-1103 = none in { $namespace }
repo-core-1104 = Notes
repo-core-1105 = Save or cancel the note first
repo-core-1106 = Namespace
repo-core-1108 = Notes are kept in refs/notes/{ $current }
repo-core-1109 = new namespace
repo-core-1110 = Use
repo-core-1111 = Reading the note…
repo-core-1112 = No note on this commit in this namespace.
repo-core-1113 = Add note
repo-core-1114 = Delete this note?
repo-core-1115 = Delete
repo-core-1116 = Edit
repo-core-1117 = Delete…
repo-core-1118 = Findings, review context, build links…
repo-core-1119 = Save note
repo-core-1120 = { $command }+Enter saves · Esc cancels
repo-core-1121 = Add a remote to fetch or push notes.
repo-core-1123 = Fetch notes
repo-core-1124 = Gets the notes in “{ $namespace }” from { $remote }. If you have notes here that { $remote } doesn't, nothing is changed and the fetch stops: your notes are kept.
repo-core-1125 = Wait for the operation running to end
repo-core-1126 = Push notes
repo-core-1127 = Sends your notes in “{ $namespace }” to { $remote }. If { $remote } has notes you don't, the push is refused.
repo-core-1128 = No notes in “{ $namespace }” to push yet
repo-core-1133 = Copy
repo-core-1134 = Could not be checked
repo-core-1135 = Check the signature again
repo-core-1136 = Signature
repo-core-1137 = Checking the signature…
repo-core-1139 = Kind
repo-core-1140 = Signer
repo-core-1141 = Key
repo-core-1142 = Primary key
repo-core-1143 = git gave no output.
repo-core-1144 = git's output
repo-core-1146 = { $count ->
                [one] { $count } file changed
               *[other] { $count } files changed
            }
repo-core-1149 = Clear
repo-core-1150 = Stash…
repo-core-1151 = Stash only the files picked
repo-core-1152 = Wait for the current operation to finish
repo-core-1153 = { $n ->
                [one] { $n } file picked to stash
               *[other] { $n } files picked to stash
            }
repo-core-1154 = Stash this file…
repo-core-1155 = { $count ->
            [one] Stash the { $count } picked file…
           *[other] Stash the { $count } picked files…
        }
repo-core-1156 = Unpick (don't stash it)
repo-core-1157 = Pick to stash ({ $command }+click)
repo-core-1159 = Cancelled before execution
repo-core-1160 = Timeline worker failed unexpectedly; inspect recovery evidence before retrying
repo-core-1162 = Behaviour timeline
repo-core-1163 = Runs on { $label } in new isolated worktrees. Commands run with your account and may execute checkout hooks/filters or access the network. Your open checkout is preserved.
repo-core-1164 = Ending revision:
repo-core-1165 = Frames:
repo-core-1166 = Loads at most 32 first-parent commits, oldest first. Results are observations of this sampled range.
repo-core-1167 = Load frames
repo-core-1168 = Command (stdout supplies the frame):
repo-core-1169 = UTF-8 output
repo-core-1170 = One benchmark number
repo-core-1171 = PNG screenshot
repo-core-1172 = Seconds per run:
repo-core-1173 = Text compares complete stdout; benchmark output must be one finite number; screenshot commands must write PNG bytes to stdout (up to 1024×1024). Each frame must repeat exactly twice in separate fresh worktrees. Exit code and complete stderr participate in repeat comparison. Limit: 2 MiB accepted per stream; transport capture is separately bounded at 64 MiB per stream.
repo-core-1174 = Run command twice per missing frame
repo-core-1175 = Stop and join owned work
repo-core-1176 = Forget tab cache / rerun
repo-core-1177 = Frame
repo-core-1178 = Open first observed adjacent change: { $hash }
repo-core-1179 = No observed adjacent stable change. Unknown or inconsistent gaps are never inferred.
repo-core-1181 = not run / evicted
repo-core-1182 = queued / running
repo-core-1183 = failed / unavailable
repo-core-1184 = inconsistent repeats
repo-core-1185 = stable
repo-core-1187 = Not run, or evicted from the bounded tab cache.
repo-core-1188 = Queued / running; no result yet.
repo-core-1189 = The two runs differ. This frame is inconsistent; no output or value is guessed.
repo-core-1190 = Exit code: { $code }
repo-core-1191 = Stderr (escaped byte preview)
repo-core-1192 = Preview shows the first 8192 bytes; complete stderr was compared.
repo-core-1195 = Exact repeated scalar; units are supplied by your command.
repo-core-1197 = Cache lives with this tab and is bounded to 64 keys / 32 MiB, keyed by repository/machine path, tree, exact command and mode. Checkout identity, timestamps or machine configuration can affect a command even with the same tree: use Forget cache when those inputs change. Closing cancels work; recovery evidence is retained if cleanup is unconfirmed.
repo-core-1198 = Stable benchmark observations (click a point; gaps are not interpolated):
repo-core-1199 = Start bisect
repo-core-1200 = Finds the commit that brought a change in (a bug, say): git checks out commits between a good one and a bad one for you to test, halving the range each time.
repo-core-1201 = { $at } is good: from before the change
repo-core-1202 = { $at } is bad: it has the change
repo-core-1203 = The bad commit, which has the change:
repo-core-1204 = HEAD, a branch, a tag or a hash
repo-core-1205 = A good commit, from before the change (or leave it empty and mark one later):
repo-core-1206 = a branch, a tag or a hash
repo-core-1207 = Start
repo-core-1208 = Run a command to bisect
repo-core-1209 = Runs it { $machine }, in the repository, at each commit git picks, until the first bad one is found. Its output shows under the banner.
repo-core-1211 = Exit code 0 marks a commit good, 125 skips it, any other up to 127 marks it bad, and 128 or more stops the run.
repo-core-1212 = Run
repo-core-1213 = Starting at { $at-label }
repo-core-1215 = Check it out
repo-core-1216 = Create
repo-core-1218 = Create branch { $name }
repo-core-1220 = At { $at-label }
repo-core-1222 = Message (optional: makes an annotated tag)
repo-core-1223 = Push it to { $remote }
repo-core-1224 = This is a release tag
repo-core-1227 = Create release tag { $name }
repo-core-1228 = Create tag { $name }
repo-core-1229 = Rename branch
repo-core-1230 = Renames { $old } on this computer; its remote copy keeps its name.
repo-core-1232 = Rename
repo-core-1233 = Rename { $old } to { $new }
repo-core-1234 = Export Git bundle
repo-core-1235 = This repository has no committed refs to export.
repo-core-1236 = Choose refs from this snapshot. The file is saved on this machine, including when the repository is on SSH / WSL.
repo-core-1238 = Optional range: exclude history reachable from this base commit
repo-core-1239 = Blank includes full reachable history. An incremental bundle requires the recipient to already have its prerequisite commits. Limit: 128 MiB; 2 minute Git deadline.
repo-core-1240 = Choose new output file…
repo-core-1242 = Bundle worker failed unexpectedly; inspect retained recovery evidence before retrying
repo-core-1243 = Inspect and import Git bundle
repo-core-1244 = Local file snapshot: { $label }
repo-core-1246 = Inspection checks the header/prerequisites only. Pack contents have not been validated until Verify succeeds.
repo-core-1247 = Header/prerequisite check: { $error }
repo-core-1248 = Verify creates a private quarantine on the repository machine and strictly checks the pack, objects and connectivity. Import repeats verification and creates only the new refs listed below. It never checks out files or overwrites existing refs. Failed/cancelled imports may leave unreferenced objects. A lost completion acknowledgement can also leave the new refs already created: inspect this group before retrying. Uncertain cleanup retains recovery evidence.
repo-core-1249 = Limits: input 128 MiB; enforced memory ceiling 1 GiB per process on Linux / 1 GiB aggregate committed memory and 8 processes on Windows; 120 seconds per Git operation; one pack thread. Linux also enforces a 512 MiB file limit. These are resource limits for Git data parsing, not a sandbox for arbitrary programs.
repo-core-1251 = This platform cannot establish the required memory containment. Unpacking/import is available on Linux, Windows, and Linux SSH/WSL hosts. Inspection and export remain available here.
repo-core-1252 = SSH/WSL import requires Python 3 and Linux resource limits. Unsupported hosts refuse before opening pack data. macOS unpacking is refused because a pre-exec address-space limit alone cannot prove enforcement after exec.
repo-core-1253 = Prerequisites ({ $count })
repo-core-1254 = None: full bundle
repo-core-1255 = New ref group (lowercase letters, digits, underscores, hyphens):
repo-core-1256 = Incoming refs to import:
repo-core-1258 = Exact new refs (existing names refuse the transaction)
repo-core-1260 = The held snapshot passed strict quarantine verification. Import will verify it again against this destination.
repo-core-1261 = Import completed. Created refs:
repo-core-1262 = Copy ref
repo-core-1263 = Stop
repo-core-1264 = Allow private quarantine creation and contained pack verification on the repository machine
repo-core-1265 = Verify complete pack
repo-core-1266 = { $count ->
            [one] Create { $count } new ref under refs/bundle-import/{ $prefix }/
           *[other] Create { $count } new refs under refs/bundle-import/{ $prefix }/
        }
repo-core-1267 = Verify again and import selected refs
repo-core-1268 = Close and refresh repository
repo-core-1269 = 1 branch
repo-core-1270 = { $count ->
            [one] { $count } branch
           *[other] { $count } branches
        }
repo-core-1271 = Delete { $count } ({ $n } on { $remotes } too)
repo-core-1272 = Delete { $count }
repo-core-1273 = Select stale or merged branches
repo-core-1274 = The ticked branches are deleted; Undo brings them back. The checked-out branch, the default branch and branches checked out in a worktree are never listed.
repo-core-1275 = Checking which branches are merged; the list fills in when done.
repo-core-1276 = No branch is merged, stale or deleted on its remote.
repo-core-1277 = Also delete their branches on the remote
repo-core-1278 = Also delete { $there } of the same name on { $remotes }
repo-core-1279 = Deleting a branch on the remote changes it for everyone. Only a remote branch named like the local one goes, never the default branch.
repo-core-1280 = None of the ticked branches has a branch of its name on a remote
repo-core-1281 = { $ticked } of { $count } ticked
repo-core-1282 = All
repo-core-1283 = None
repo-core-1285 = also on { $remote }/{ $name }
repo-core-1287 = Runs
repo-core-1289 = Custom command
repo-core-1292 = Repository commands
repo-core-1293 = These come from { $REPO-FILE } in { $repo }. They run on the repository's machine as you: trust them only if you trust whoever can change this file.
repo-core-1295 = It lists no commands.
repo-core-1296 = { $label } · output: { $label2 }
repo-core-1297 = Runs at once when picked, without asking
repo-core-1298 = Asks for ${ $variable } ({ $label })
repo-core-1299 = , “{ $default }” by default
repo-core-1300 = Trust and use
repo-core-1301 = The staged file
repo-core-1302 = The { $staged } staged files
repo-core-1303 = Commit staged changes as a fixup
repo-core-1304 = is committed as a fixup of
repo-core-1305 = are committed as a fixup of
repo-core-1306 = Fold staged changes into a commit
repo-core-1307 = goes into
repo-core-1308 = go into
repo-core-1309 = { $files } { $lead } this commit:
repo-core-1310 = Keep as fixup! commit for later
repo-core-1311 = Commit the staged changes as “fixup! <summary>” on top of the branch, and squash it later (git rebase -i --autosquash)
repo-core-1312 = A commit “fixup! { $summary }” goes on top of { $branch }; nothing is rewritten. An interactive rebase with autosquash squashes it later.
repo-core-1313 = The commit keeps its message. Unstaged and untracked changes stay as they are. Ctrl+Z undoes it.
repo-core-1314 = Commit as fixup
repo-core-1315 = Fold
repo-core-1316 = It is the latest commit: no other commit is rewritten.
repo-core-1317 = The commit after it on { $branch } is rewritten too:
repo-core-1318 = { $n ->
        [one] The { $n } commit after it on { $branch } is rewritten too:
       *[other] The { $n } commits after it on { $branch } are rewritten too:
    }
repo-core-1319 = … and { $LISTED } older ones
repo-core-1320 = This commit is already pushed
repo-core-1321 = { $total ->
        [one] { $total } commit is already pushed
       *[other] All { $total } commits are already pushed
    }
repo-core-1322 = { $n ->
                [one] { $n } of these commits is already pushed
               *[other] { $n } of these commits are already pushed
            }
repo-core-1323 = is
repo-core-1324 = are
repo-core-1325 = { $what }: folding rewrites shared history, and the branch will need a force push.
repo-core-1326 = pushed
repo-core-1327 = A remote branch already has this commit
repo-core-1328 = Submit { $min } to Gerrit on { $remote }.
repo-core-1329 = Target branch
repo-core-1331 = Topic (optional)
repo-core-1332 = Group related changes
repo-core-1333 = Fetch the remote's target branches first.
repo-core-1335 = Checks every commit above the fetched target for a unique Change-Id. Amend a change, keep its Change-Id, then push here again to upload a new patch set.
repo-core-1336 = The pre-push hook will be skipped for this submission.
repo-core-1337 = The topic cannot contain control characters.
repo-core-1338 = Find the interaction
repo-core-1339 = For two branches that pass alone but fail together: tests subsets of their commits { $machine }, in a scratch worktree, until it finds the ones that fail together. The two may have up to { $MAX-CHANGES } commits together since they diverged, and no merges.
repo-core-1340 = The two branches:
repo-core-1342 = Command (exit code 0 passes):
repo-core-1343 = Tests at most:
repo-core-1344 = Out of tests, it shows the smallest failing set it got to
repo-core-1345 = A failure is run again:
repo-core-1346 = A failure that doesn't repeat is set aside as flaky
repo-core-1347 = more time(s) to be sure
repo-core-1348 = Find
repo-core-1349 = Keep the commits made here?
repo-core-1350 = and more
repo-core-1351 = Branch to keep them on
repo-core-1352 = branch name
repo-core-1353 = A branch of that name exists already
repo-core-1354 = Keep and check out
repo-core-1355 = Create the branch { $trim } at these commits, then check { $target } out
repo-core-1356 = Leave them
repo-core-1357 = Check { $target } out; the commits stay on no branch
repo-core-1358 = More than { $n } commits made here
repo-core-1360 = The commit made here
repo-core-1362 = { $n ->
        [one] The { $n } commit made here
       *[other] The { $n } commits made here
    }
repo-core-1363 = { $more ->
                [yes] More than { $count } commits made here are on no branch. Checking out { $target } leaves them where nothing shows them; git only keeps them in its reflog, for a while.
               *[no] { $count ->
                    [one] The commit made here is on no branch. Checking out { $target } leaves it where nothing shows it; git only keeps it in its reflog, for a while.
                   *[other] The { $count } commits made here are on no branch. Checking out { $target } leaves them where nothing shows them; git only keeps them in its reflog, for a while.
                }
            }
repo-core-1364 = Linked-copy inspection failed unexpectedly
repo-core-1365 = Same code across linked copies
repo-core-1366 = Save up to eight local, WSL or SSH repository paths. Inspect connects to those copies and runs Git plus five fixed tool-version commands. It reads tracked-file metadata, six Git settings and existing default git-test results. It never requests environment variables, credential settings, remote URLs or file contents.
repo-core-1367 = Unlink
repo-core-1368 = Absolute local path, //wsl.localhost/Distro/path, or ssh://host/absolute/path
repo-core-1369 = Save link
repo-core-1370 = Inspect linked copies
repo-core-1371 = Reading copies; closing this dialog cancels its jobs.
repo-core-1372 = Open copy
repo-core-1373 = Unavailable: { $e }
repo-core-1374 = Default test commands differ; stored outcomes are not comparable.
repo-core-1375 = Reference copy unavailable; content comparison unknown.
repo-core-1376 = { $machine } · HEAD { $head } · stored default test: { $untested }
repo-core-1377 = untested
repo-core-1378 = Stored notes can be stale or externally written; inspection does not run tests or certify their command provenance.
repo-core-1379 = Allowlisted facts and differences
repo-core-1380 = { $count ->
            [one] { $count } tracked path · tree { $tree }
           *[other] { $count } tracked paths · tree { $tree }
        }
repo-core-1382 = Potential case collisions (Unicode lowercase comparison):
repo-core-1383 = Working-tree mode changes:
repo-core-1384 = { $path } · mode { $mode } · { $endings }
repo-core-1385 = Matches compare clean tracked Git blob identities. Untracked files are excluded. Symlink targets, submodule worktrees and byte-for-byte checkout equivalence are not certified. Limits: 20,000 paths, 4 MiB per facts response, 120 seconds per facts command.
repo-core-1386 = Tracked Git content matches the first copy (modes and checkout line endings may differ).
repo-core-1387 = Tracked Git content differs from the first copy.
repo-core-1388 = Tracked changes, skipped index paths or a concurrent change: matching content is unconfirmed.
repo-core-1389 = Object formats differ: matching content is unconfirmed.
repo-core-1391 = on { $label }
repo-core-1392 = on this machine
repo-core-1394 = Checking outgoing history…
repo-core-1395 = Local/fetched objects only; nothing is pushed until you review.
repo-core-1396 = Push preflight could not finish
repo-core-1397 = Reading earlier versions…
repo-core-1398 = Reading bundle information…
repo-core-1399 = Git runs on the repository's machine. Local bundle files are temporarily copied there for SSH / WSL verification. No refs change while inspecting.
repo-core-1400 = Discard this hunk?
repo-core-1401 = This removes the current hunk from the working tree. Ctrl+Z restores it.
repo-core-1402 = Discard hunk
repo-core-1403 = Type the bad commit, which has the problem
repo-core-1404 = Type a good commit, one from before the problem
repo-core-1405 = A good commit can't start with -
repo-core-1406 = Type the command that tests a commit
repo-core-1407 = Choose how many commits to test at once
repo-core-1408 = Bisect in parallel
repo-core-1409 = Tests several commits at once, each in a worktree of its own on the machine chosen, until the first bad commit is found: about log base (k + 1) of n rounds instead of log base 2 of n. Your checkout isn't touched.
repo-core-1410 = Test command, run at the top of each worktree:
repo-core-1411 = As git bisect run: exit code 0 is good, 125 skips, any other up to 127 is bad. 128 or more (a crash) stops the bisect on an end, and skips any other commit.
repo-core-1412 = Bad commit, which has the problem:
repo-core-1413 = Good commits, from before it (separated by spaces):
repo-core-1414 = a tag, a branch or a hash
repo-core-1415 = The commit the bisect skipped isn't tested.
repo-core-1416 = { $n ->
        [one] The { $n } commit the bisect skipped isn't tested.
       *[other] The { $n } commits the bisect skipped aren't tested.
    }
repo-core-1417 = Run on:
repo-core-1418 = { $label } (where the repository is)
repo-core-1420 = Looking for WSL distributions…
repo-core-1421 = A clone of this repository there (optional):
repo-core-1422 = empty: gitcraft keeps a copy in ~/.cache/gitcraft/bisect
repo-core-1423 = The commits it lacks are sent there first.
repo-core-1424 = At once:
repo-core-1425 = How many commits are tested side by side, each in its own worktree
repo-core-1426 = { $n ->
            [one] { $n } core on { $label }
           *[other] { $n } cores on { $label }
        }
repo-core-1427 = cores unknown
repo-core-1428 = counting cores…
repo-core-1429 = Time limit:
repo-core-1430 = { " " }min
repo-core-1431 = A test that runs longer is stopped and counts as skipped
repo-core-1432 = Uncommitted changes lost
repo-core-1433 = Its commits
repo-core-1434 = Commits not on { $head }
repo-core-1435 = Reading what this changes…
repo-core-1436 = Couldn't read what this changes: { $e }
repo-core-1437 = { $heading }: none
repo-core-1439 = and { $more } more
repo-core-1440 = only here
repo-core-1441 = still on { $name }
repo-core-1442 = still on another branch
repo-core-1443 = and { $more } more, all only here
repo-core-1444 = and { $more } more, { $n } of them only here
repo-core-1445 = 1 commit this rewrites is
repo-core-1446 = { $commits } commits this rewrites are
repo-core-1447 = { $commits ->
            [one] Already pushed: 1 commit this rewrites is on { $upstream }, so pushing it again will need a force push.
           *[other] Already pushed: { $commits } commits this rewrites are on { $upstream }, so pushing it again will need a force push.
        }
repo-core-1448 = Git commands
repo-core-1451 = { $key }+Z moves the branch back, not the uncommitted changes.
repo-core-1452 = { $key }+Z restores this.
repo-core-1453 = Rehearse
repo-core-1454 = Plays it through { $machine } in a scratch worktree and says where it stops, which files conflict and whether the tests pass. Nothing of yours changes until you pick one.
repo-core-1455 = Check out a branch first: rehearsals are of the checked-out one.
repo-core-1456 = Rebase { $current } onto { $other }
repo-core-1457 = Merge { $other } into { $current }
repo-core-1458 = the merge
repo-core-1459 = the rebase
repo-core-1460 = Rehearse { $the-other } too, to compare
repo-core-1461 = Test the result with (leave empty not to):
repo-core-1462 = Edit remote
repo-core-1463 = Add remote
repo-core-1464 = A name (like origin or upstream) and the repository's URL.
repo-core-1468 = Remote { $name }
repo-core-1469 = Sign in to { $host }
repo-core-1470 = git needs a username and password for this remote. Most hosts want a personal access token as the password.
repo-core-1471 = Git did not identify the failing endpoint. Choose the destination this sign-in should authorize. Other endpoints will not receive this token.
repo-core-1473 = Choose destination
repo-core-1474 = Endpoint: { $url }
repo-core-1475 = The server refused the sign-in. A 401 or 403 immediately after signing in often means the wrong account is stored, or that this account lacks repository access.
repo-core-1476 = Forget stored account and switch…
repo-core-1477 = Username
repo-core-1478 = Password or token
repo-core-1479 = Used only for this HTTPS endpoint and account until you close the tab. Requires Git 2.46 or newer on the repository machine. Supplies the selected username/password before the first request and is not automatically saved to a credential manager.
repo-core-1480 = An exact HTTPS endpoint is required for transient sign-in. Other transports can use the profile credential manager.
repo-core-1481 = Sign in
repo-core-1482 = Create { $noun }
repo-core-1483 = { $branch } → { $base } (the branch is pushed first)
repo-core-1484 = Title
repo-core-1485 = Description
repo-core-1486 = Into
repo-core-1487 = Draft
repo-core-1488 = the remote
repo-core-1489 = checking which git this repository has…
repo-core-1490 = nothing is staged
repo-core-1491 = { $names } and { $more } more
repo-core-1492 = Stash changes
repo-core-1493 = Saves changes in a stash; Undo takes it back.
repo-core-1494 = Everything
repo-core-1495 = Every change; the files go back to how the last commit has them.
repo-core-1496 = { $command }+click files in the changes to pick them
repo-core-1497 = Picked files
repo-core-1498 = { $n ->
                [one] { $n } picked file
               *[other] { $n } picked files
            }
repo-core-1499 = Only what is staged
repo-core-1500 = The staged changes; the unstaged ones stay.
repo-core-1501 = Snapshot
repo-core-1502 = Saves a stash and leaves every file and the staging as they are.
repo-core-1503 = Message (optional)
repo-core-1504 = Save snapshot
repo-core-1505 = Include untracked files
repo-core-1506 = Untracked files you picked are stashed; the others stay
repo-core-1507 = Untracked files aren't staged, so they stay
repo-core-1508 = Test commits
repo-core-1509 = Runs the command { $machine } at each commit, in scratch worktrees, in the background, and shows a pass / fail square by each. Results are kept by tree, as git-test keeps them.
repo-core-1510 = Commits (base..tip, or one commit):
repo-core-1512 = Command, run at the top of the worktree:
repo-core-1513 = Exit code 0 passes; any other fails.
repo-core-1514 = Commits at once:
repo-core-1515 = How many commits are tested at the same time, each in its own worktree
repo-core-1516 = At most { $REMOTE-WORKERS } at once on another machine, to keep its connections free for the rest of the window
repo-core-1517 = Test again commits that have a result
repo-core-1518 = Forget the results of the old command (`{ $old }`); this can't be undone
repo-core-1519 = Its results stay, and show as the new command's: commits with one are not tested again unless you tick Test again above
repo-core-1520 = Type the command to test with
repo-core-1521 = Name the commits to test: a range can't be empty or start with -
repo-core-1522 = Set upstream
repo-core-1523 = The upstream { $branch } pulls from. Its push route is configured separately.
repo-core-1525 = Branch on the remote
repo-core-1527 = Pick an existing branch
repo-core-1528 = Tracks the existing { $remote }/{ $target }.
repo-core-1529 = { $remote }/{ $target } doesn't exist yet: pushes { $branch } there to create it.
repo-core-1532 = Track { $upstream }
repo-core-1533 = Push { $branch } to { $upstream }
repo-core-1535 = Add worktree
repo-core-1536 = A second working folder of this repository, on its own branch.
repo-core-1537 = Existing branch
repo-core-1538 = New branch
repo-core-1539 = Default worktrees root
repo-core-1540 = Worktrees root
repo-core-1541 = Beside repository
repo-core-1542 = Inside repository
repo-core-1544 = Saved when you add. Relative to the main checkout; shared by its worktrees.
repo-core-1545 = Folders inside a checkout are automatically ignored locally.
repo-core-1546 = Worktree folder
repo-core-1547 = Use branch folder
repo-core-1549 = Starting at { $base }
repo-core-1551 = Choose a branch
repo-core-1552 = Every branch is checked out somewhere
repo-core-1553 = Type a path on that machine
repo-core-1554 = Algorithm
repo-core-1555 = { $ext } files
repo-core-1556 = files named { $name }
repo-core-1558 = Default
repo-core-1559 = What your git config says (diff.algorithm), else Myers
repo-core-1560 = Diff algorithm: how the lines of both sides are paired. Kept for { $kept-for }; the repository's config is left as it is.
repo-core-1561 = Reading the diff again with { $label }; the lines shown are paired the old way
repo-core-1562 = git's default
repo-core-1563 = { "" }
            … (the rest of the text isn't shown)
repo-core-1564 = Before
repo-core-1565 = After
repo-core-1567 = Binary
repo-core-1568 = size unavailable
repo-core-1571 = { $name } · { $old } → { $new } bytes
repo-core-1573 = PDF document
repo-core-1576 = Word document
repo-core-1579 = Excel workbook
repo-core-1582 = PowerPoint presentation
repo-core-1584 = OpenDocument text
repo-core-1586 = OpenDocument spreadsheet
repo-core-1588 = OpenDocument presentation
repo-core-1590 = Java archive
repo-core-1592 = Android package
repo-core-1594 = EPUB book
repo-core-1596 = Unity package
repo-core-1597 = ZIP archive
repo-core-1598 = Members
repo-core-1599 = Unpacked
repo-core-1600 = Binary file
repo-core-1602 = ELF executable or library
repo-core-1604 = Windows executable or library
repo-core-1606 = Mach-O executable
repo-core-1608 = Mach-O universal binary or Java class
repo-core-1610 = SQLite database
repo-core-1612 = gzip archive
repo-core-1614 = bzip2 archive
repo-core-1616 = xz archive
repo-core-1618 = 7-Zip archive
repo-core-1620 = RAR archive
repo-core-1622 = Zstandard archive
repo-core-1624 = Ogg audio or video
repo-core-1626 = FLAC audio
repo-core-1628 = MP3 audio
repo-core-1630 = WebAssembly module
repo-core-1632 = Office 97–2003 document
repo-core-1634 = WOFF font
repo-core-1636 = WOFF2 font
repo-core-1638 = TrueType font
repo-core-1640 = OpenType font
repo-core-1642 = glTF binary model
repo-core-1644 = FBX model
repo-core-1646 = Photoshop image
repo-core-1648 = RIFF media (WAV, AVI…)
repo-core-1649 = Dimensions
repo-core-1650 = { $w } × { $h }
repo-core-1652 = grey
repo-core-1653 = RGB
repo-core-1654 = indexed
repo-core-1655 = grey with alpha
repo-core-1656 = RGB with alpha
repo-core-1657 = unknown
repo-core-1658 = { $b } bits, { $color }
repo-core-1659 = PNG image
repo-core-1660 = Colour
repo-core-1663 = GIF image
repo-core-1665 = BMP image
repo-core-1666 = JPEG image
repo-core-1667 = Version
repo-core-1668 = Pages
repo-core-1671 = Author
repo-core-1673 = Made with
repo-core-1683 = Text
repo-core-1684 = Bytes
repo-core-1685 = Too big to show: gitcraft reads files up to { $MAX-READ }.
repo-core-1687 = same size
repo-core-1688 = couldn't be read
repo-core-1689 = { $n }, not read (over { $limit })
repo-core-1690 = over { $limit }, not read
repo-core-1693 = Size
repo-core-1695 = (new file)
repo-core-1696 = (deleted)
repo-core-1697 = No text.
repo-core-1698 = The text is the same; see { $more } for what changed.
repo-core-1700 = No member changed.
repo-core-1702 = added
repo-core-1703 = removed
repo-core-1704 = changed
repo-core-1705 = { $n }, same size
repo-core-1706 = Showing the first 64 KB of each side.
repo-core-1711 = Text made by its textconv,
repo-core-1713 = not the file's own lines. Unstage it as a whole file.
repo-core-1714 = not the file's own lines. Mark it resolved as a whole file.
repo-core-1715 = not the file's own lines. Stage or discard it as a whole file.
repo-core-1716 = not the file's own lines.
repo-core-1724 = Malformed or unsupported SOPS document. Showing the encrypted patch remains available.
repo-core-1734 = Ask when ambiguous
repo-core-1735 = JSON document
repo-core-1736 = Binary envelope
repo-core-1737 = SOPS comparison needs UTF-8 text within 2 MiB per side.
repo-core-1745 = SOPS comparison does not support YAML aliases or anchors. Use the encrypted patch.
repo-core-1746 = SOPS document structure exceeded its limit.
repo-core-1758 = Syntax preparation timed out. Showing the patch.
repo-core-1759 = Encrypted source exceeds size limit
repo-core-1760 = Syntax metadata is not UTF-8
repo-core-1768 = This file is no longer in the comparison. Refresh its diff.
repo-core-1769 = Syntax comparison is not available for merge-resolution diffs. Showing the patch.
repo-core-1773 = Resolve the conflict before comparing its syntax.
repo-core-1776 = Couldn't read this blob's size
repo-core-1777 = Syntax comparison is limited to 2 MiB per file.
repo-core-1778 = Encrypted source is not a safely readable regular file within 2 MiB
repo-core-1779 = Syntax comparison is available for text files only.
repo-core-1780 = Syntax comparison needs UTF-8 text.
repo-core-1781 = Private decrypted comparison · read only
repo-core-1782 = Plaintext is held only by this view. Closing, refreshing, changing modes or leaving this tab discards it. Return to the encrypted patch to stage changes.
repo-core-1783 = Decrypting with local SOPS and keys on this computer… Cancel returns to the encrypted patch.
repo-core-1784 = Choose Encrypted patch to return, or Show decrypted again to retry.
repo-core-1785 = Wrap
repo-core-1786 = Decrypted content: changed
repo-core-1787 = Decrypted content: unchanged
repo-core-1788 = Recipients / wrapped keys: changed
repo-core-1789 = Recipients / wrapped keys: unchanged
repo-core-1790 = Other encryption metadata changed
repo-core-1791 = No plaintext changes. Ciphertext or integrity metadata may have changed through re-encryption.
repo-core-1795 = Show the unchanged lines right above this hunk
repo-core-1797 = Show the unchanged lines right below the hunk above
repo-core-1798 = Show all { $hidden }
repo-core-1799 = Show every unchanged line folded here
repo-core-1800 = Function
repo-core-1801 = Show the enclosing function: its unchanged lines above and below this hunk
repo-core-1802 = 1 unchanged line
repo-core-1803 = { $hidden ->
            [one] { $hidden } unchanged line
           *[other] { $hidden } unchanged lines
        }
repo-core-1807 = Fit
repo-core-1808 = Fit the picture (double-click it)
repo-core-1809 = One picture pixel a screen pixel
repo-core-1810 = All new
repo-core-1811 = All old
repo-core-1812 = or drag the line
repo-core-1813 = Old
repo-core-1814 = New
repo-core-1817 = Working out the difference…
repo-core-1818 = Image comparison zoom for { $path }. Pan position { NUMBER($x, minimumFractionDigits: 0, maximumFractionDigits: 0) }%, { NUMBER($y, minimumFractionDigits: 0, maximumFractionDigits: 0) }%. Arrow keys pan; plus and minus zoom; Home fits. Use controls above to change comparison mode.
repo-core-1819 = { " " }(shown smaller)
repo-core-1820 = { $width } × { $height }{ $scaled }
repo-core-1821 = { $b }, same dimensions
repo-core-1822 = no pixel changed
repo-core-1823 = 1 pixel changed
repo-core-1824 = { $n ->
            [one] { $n } pixel changed
           *[other] { $n } pixels changed
        }
repo-core-1825 = { NUMBER($share, minimumFractionDigits: 1, maximumFractionDigits: 1) }% of pixels changed
repo-core-1830 = Image version { $hash }, { $summary }, { $path }
repo-core-1832 = (none)
repo-core-1834 = 2-up
repo-core-1835 = Swipe
repo-core-1836 = Onion skin
repo-core-1837 = Difference
repo-core-1849 = Isolated SVG preview: external resources disabled; bounded embedded PNG/JPEG images only. Unsupported effects can be inspected in SVG source.
repo-core-1851 = { $div-ceil } KB
repo-core-1854 = Stored in Git LFS
repo-core-1855 = LFS object { $oid }
repo-core-1856 = Untracked and conflicted files have no line history
repo-core-1857 = Select some lines first: drag over their text
repo-core-1860 = { $label } (before the change)
repo-core-1864 = Not a notebook (its JSON doesn't read: { $e })
repo-core-1866 = A notebook of format { $major }: gitcraft reads format 4
repo-core-1868 = A notebook without cells
repo-core-1881 = Error
repo-core-1884 = { $at }
            … { $more } more lines
repo-core-1894 = Cell { $number } · { $status }
repo-core-1895 = { $label } · { $label2 } · execution { $execution-count }
repo-core-1897 = Image output ({ $mime }); pixels are not searched
repo-core-1898 = Unsupported output: { $kinds }
repo-core-1900 = Cells
repo-core-1902 = Outputs and execution counts aren't compared: they are stripped when staged.
repo-core-1903 = Outputs stripped when committing (by the { $name } filter)
repo-core-1904 = Set up outside gitcraft (nbstripout --install, or the repository's .gitattributes).
repo-core-1905 = { " " }It runs nbstripout.
repo-core-1906 = { " " }It runs Python.
repo-core-1907 = Strip outputs when committing
repo-core-1908 = For this clone only: what is staged of a notebook has no outputs, execution counts or widget state (a clean filter, like nbstripout), while the file on disk keeps them. Notebooks that have outputs show as changed until they are committed once so.{ $tool }
repo-core-1909 = { $n } { $cells } { $what }
repo-core-1910 = cell
repo-core-1911 = { $n } { $what }
repo-core-1912 = Only the notebook's metadata changed
repo-core-1913 = No cell changed
repo-core-1915 = The notebook's metadata changed
repo-core-1917 = 1 unchanged cell
repo-core-1918 = { $count ->
            [one] { $count } unchanged cell
           *[other] { $count } unchanged cells
        }
repo-core-1919 = { $label } cell { $number }
repo-core-1920 = moved
repo-core-1921 = { $kind } cell { $number }
repo-core-1924 = metadata changed
repo-core-1925 = Metadata
repo-core-1927 = Outputs (removed)
repo-core-1928 = Outputs (added)
repo-core-1929 = Outputs
repo-core-1931 = Outputs before
repo-core-1932 = Outputs after
repo-core-1935 = Traceback
repo-core-1936 = (an output gitcraft can't show)
repo-core-1937 = ({ $kinds } output)
repo-core-1938 = { $verb } hunk
repo-core-1940 = Stage this hunk. Click lines to stage only some of them (Shift+click for a range).
repo-core-1941 = Unstage this hunk. Click lines to unstage only some of them (Shift+click for a range).
repo-core-1942 = Throw away this hunk's changes (Ctrl+Z brings them back).
repo-core-1943 = Undo this hunk in the working tree, to review before committing.
repo-core-1944 = … diff truncated
repo-core-1952 = resolves a conflict
repo-core-1953 = edited beyond the merge
repo-core-1954 = git's merge → merge { $hash }
repo-core-1955 = No line differs from git's own merge but in whitespace.
repo-core-1956 = The recorded merge keeps this file as git's own merge leaves it: no line differs.
repo-core-1963 = Couldn't read the file on either side of the change
repo-core-1968 = The change leaves the rendered text as it was.
repo-core-1969 = Removed
repo-core-1970 = Added
repo-core-1985 = it isn't an OpenAPI or Swagger document (no `openapi` or `swagger` key at its top)
repo-core-1986 = The whole API document was deleted
repo-core-1989 = Endpoint removed
repo-core-1990 = Endpoint added
repo-core-1996 = { $owner } field `{ $path }`
repo-core-1997 = { $place } parameter `{ $name }`
repo-core-1998 = New required { $place } parameter `{ $name }`
repo-core-1999 = { $label } is now required
repo-core-2000 = { $place } parameter `{ $name }` was removed (clients sending it may be refused)
repo-core-2001 = A request body is now required
repo-core-2002 = The request body is now required
repo-core-2003 = The request body no longer accepts `{ $m }`
repo-core-2004 = Request body
repo-core-2007 = Response `{ $key }` was removed
repo-core-2008 = Response `{ $key }`
repo-core-2010 = { $name } no longer accepts other fields
repo-core-2011 = { $name } was removed
repo-core-2013 = { $name } is no longer always returned
repo-core-2014 = { $name } is now required
repo-core-2015 = { $owner }: new required field `{ $path }`
repo-core-2018 = { $name } changed type: { $ot } → { $nt }
repo-core-2021 = { $name } now accepts only { $only }
repo-core-2022 = { $name } no longer accepts { $missing }
repo-core-2023 = { $name } can now return { $extra }
repo-core-2032 = API contract · { $version }
repo-core-2033 = no change to the endpoints, parameters or schemas clients use
repo-core-2034 = nothing breaks · { $n } added
repo-core-2035 = 1 breaking change · { $n } added
repo-core-2036 = { $b } breaking changes · { $n } added
repo-core-2039 = Go to line { $n }
repo-core-2040 = Go to line { $o } of the file before the change
repo-core-2047 = not a config file
repo-core-2048 = the { $format } { $side } the change isn't valid ({ $e })
repo-core-2049 = before
repo-core-2050 = after
repo-core-2051 = (the whole file)
repo-core-2053 = { $format } by key
repo-core-2054 = the same keys and values: only formatting or comments changed
repo-core-2055 = Show the API contract changes
repo-core-2056 = What breaks this API's clients, and what was added
repo-core-2060 = { "   " }moved, same value
repo-core-2062 = Go to the lines
repo-core-2063 = Invalid package archive
repo-core-2064 = Expanded package exceeds 32 MiB
repo-core-2065 = Invalid archive header
repo-core-2066 = Invalid archive checksum
repo-core-2067 = Package archive checksum mismatch
repo-core-2068 = Invalid archive size
repo-core-2069 = Archive overflow
repo-core-2070 = Truncated package archive
repo-core-2071 = Invalid long path
repo-core-2073 = Unsafe package member path
repo-core-2079 = Duplicate package metadata member
repo-core-2080 = included, but exceeds the 512 KiB text limit ({ $size } bytes)
repo-core-2081 = included, but exceeds the retained metadata count/byte limit
repo-core-2082 = included, but not UTF-8 text
repo-core-2106 = PyPI's release history
repo-core-2109 = not a lockfile this reads
repo-core-2138 = the { $file } { $side } the change isn't valid ({ $e })
repo-core-2139 = Dependencies · { $file }
repo-core-2140 = upgraded
repo-core-2141 = downgraded
repo-core-2142 = with a new licence
repo-core-2143 = the same packages and versions
repo-core-2145 = licence { $o } → { $n }
repo-core-2146 = licence { $n }
repo-core-2147 = licence { $o } → none given
repo-core-2148 = Go to line { $o } of the lockfile before the change
repo-core-2149 = Go to line { $n } of the lockfile
repo-core-2150 = Package { $name }. { $tip }
repo-core-2151 = what changed
repo-core-2153 = Changelog & licences
repo-core-2166 = it tracks nothing (no `outs` or `deps` with a `path`)
repo-core-2167 = the .dvc file { $side } the change isn't valid ({ $e })
repo-core-2169 = Data tracked by DVC
repo-core-2170 = Download data for the current checkout from its configured DVC remote
repo-core-2171 = now tracked
repo-core-2172 = no longer tracked
repo-core-2173 = Go to its line
repo-core-2174 = Go to line { $line } for tracked data { $path }, { $state }
repo-core-2175 = imported from { $url }
repo-core-2176 = where the data was imported from
repo-core-2177 = not tracked
repo-core-2178 = imported
repo-core-2179 = size not recorded
repo-core-2180 = { $files ->
                [one] a folder of { $files } file
               *[other] a folder of { $files } files
            }
repo-core-2182 = more after the end of the document
repo-core-2183 = line { $line }: { $what }
repo-core-2184 = a comment that never ends
repo-core-2185 = expected `{ $char }`
repo-core-2186 = expected a value
repo-core-2187 = the document ends where a value should be
repo-core-2188 = nested deeper than { $MAX-DEPTH } levels
repo-core-2191 = `{ $text }` isn't a number
repo-core-2192 = a string that never ends
repo-core-2193 = a control character (a line break, a tab) inside a string
repo-core-2195 = an unknown escape in a string
repo-core-2196 = a `\u` escape without four hex digits
repo-core-2197 = expected a key in quotes or `{ "}" }`
repo-core-2198 = expected `,` or `{ "}" }`
repo-core-2199 = expected `,` or `]`
repo-core-2205 = The file is empty on both sides.
repo-core-2206 = The whole file is new.
repo-core-2207 = The change deletes the whole file.
repo-core-2208 = The words are as they were: only whitespace, blank lines or the file's name or mode changed.
repo-core-2209 = The words are as they were: only how the lines are wrapped changed.
repo-core-2213 = 1 unchanged paragraph
repo-core-2214 = { $count ->
            [one] { $count } unchanged paragraph
           *[other] { $count } unchanged paragraphs
        }
repo-core-2216 = Show them
repo-core-2217 = re-wrapped
repo-core-2218 = edited
repo-core-2220 = Line { $o } before the change: click to see the lines
repo-core-2221 = Line { $n }: click to see the lines there
repo-core-2222 = { " " }… ({ $MAX-CHARS } more characters)
repo-core-2223 = Added · { $text }
repo-core-2224 = Removed · { $text }
repo-core-2225 = Before · { $old }
repo-core-2226 = After · { $new }
repo-core-2227 = Type · { $old } → { $new }
repo-core-2228 = absent
repo-core-2229 = Licence · { $declared } → { $declared2 }
repo-core-2230 = not declared
repo-core-2231 = Column · { $column } → { $column2 }
repo-core-2232 = Before · { $old }
repo-core-2233 = After · { $new }
repo-core-2234 = { $files ->
            [one] { $side }: a folder of { $files } file
           *[other] { $side }: a folder of { $files } files
        }
repo-core-2236 = hash not recorded
repo-core-2237 = over { $MAX-CELLS } cells
repo-core-2238 = line { $quote-line }: a quoted cell that never ends
repo-core-2244 = not a table
repo-core-2245 = (column { $k })
repo-core-2246 = { $format } as a table
repo-core-2247 = row
repo-core-2248 = column
repo-core-2249 = renamed
repo-core-2250 = { $n } { $what }{ $s } { $how }
repo-core-2251 = the same cells: only quoting, line endings or blank lines changed
repo-core-2252 = rows matched by their first column
repo-core-2253 = Its values are all there and all different, so each row is found by it however the rows moved
repo-core-2255 = 1 unchanged row
repo-core-2256 = { $count ->
            [one] { $count } unchanged row
           *[other] { $count } unchanged rows
        }
repo-core-2258 = Show { $label }
repo-core-2260 = Column added
repo-core-2261 = Column removed
repo-core-2262 = Column renamed
repo-core-2267 = line { $line }: { $trim }
repo-core-2268 = a key went missing
repo-core-2269 = string
repo-core-2270 = boolean
repo-core-2271 = date
repo-core-2272 = alias
repo-core-2276 = { "{" }1 key{ "}" }
repo-core-2277 = { $n ->
            [one] { "{" }{ $n } key{ "}" }
           *[other] { "{" }{ $n } keys{ "}" }
        }
repo-core-2278 = [1 item]
repo-core-2279 = { $n ->
            [one] [{ $n } item]
           *[other] [{ $n } items]
        }
repo-core-2281 = line { $line }: { $info }
repo-core-2282 = line { $line }: nested deeper than { $MAX-DEPTH } levels
repo-core-2299 = (alias)
repo-core-2303 = Diff change map. Arrow keys scroll a line; Page Up and Page Down scroll a page; Home and End reach the ends.
repo-core-2306 = Prompt step
repo-core-2308 = Read local recorded edits matching this immutable commit hunk
repo-core-2309 = Moved to line { $false }
repo-core-2310 = Moved from line { $true }
repo-core-2311 = { $label }, edited
repo-core-2313 = Show both places of the moved block, and what changed on the way
repo-core-2314 = Comment on this line
repo-core-2315 = Note for the agent on this line
repo-core-2316 = Alt+click: note for the agent on this line
repo-core-2318 = { $count ->
            [one] { $tip }, line { $number }, { $count } comment
           *[other] { $tip }, line { $number }, { $count } comments
        }
repo-core-2319 = deleted
repo-core-2320 = hunk
repo-core-2321 = context
repo-core-2322 = { $kind }, old line { $line }, new line { $line2 }: { $text }
repo-core-2323 = none
repo-core-2324 = Select text to copy first
repo-core-2325 = Explain history
repo-core-2326 = Symbol biography
repo-core-2327 = History of these lines
repo-core-2328 = Every commit up to this version that changed the selected lines, followed back through earlier edits and renames
repo-core-2329 = Difftastic returned an incompatible syntax report. Update difft and try again.
repo-core-2330 = Difftastic doesn't support this file's language.
repo-core-2332 = Difftastic used a text diff: { $language }.
repo-core-2333 = Difftastic classified this file as binary.
repo-core-2335 = This syntax comparison has too many lines to show.
repo-core-2336 = A line is too long for syntax comparison.
repo-core-2337 = Syntax comparison is currently available for local repositories. Showing the patch.
repo-core-2338 = Couldn't read a regular file within 2 MiB from the working tree (links are not followed).
repo-core-2339 = { $language } · syntax changes · read only
repo-core-2340 = Use the patch to stage changes and to see formatting edits.
repo-core-2341 = No syntax changes. Formatting can still differ in the patch.
repo-core-2343 = Syntax change { $n }
repo-core-2344 = SOPS encrypted file
repo-core-2345 = Decryption compares the complete file versions, not selected history lines.
repo-core-2347 = Encrypted patch / cancel
repo-core-2348 = Show decrypted
repo-core-2349 = Use local SOPS and keys. SSH/WSL supply only ciphertext. Plaintext is discarded when leaving this view.
repo-core-2354 = Showing the patch. Turn off Syntax to stage changes.
repo-core-2355 = Commit { $hash }
repo-core-2356 = Staged changes
repo-core-2357 = Unstaged changes
repo-core-2358 = New file
repo-core-2359 = Conflicted
repo-core-2360 = before the change
repo-core-2361 = Three-state
repo-core-2362 = Moves across files
repo-core-2363 = Show the Markdown as text
repo-core-2364 = Show the changes rendered
repo-core-2365 = Show the file rendered
repo-core-2367 = Diff
repo-core-2368 = File
repo-core-2369 = Unstage file
repo-core-2370 = Stage file
repo-core-2371 = Keep both…
repo-core-2372 = For a file both sides changed that can't be merged (an image, an archive…): write each side's version as a file of its own, named after its branch. Shows what it would write first.
repo-core-2373 = Keep both is already open for a file
repo-core-2376 = { $verb ->
                [Stage] { $chosen ->
                [one] Stage { $chosen } line
               *[other] Stage { $chosen } lines
            }
                [Unstage] { $chosen ->
                [one] Unstage { $chosen } line
               *[other] Unstage { $chosen } lines
            }
                [Discard] { $chosen ->
                [one] Discard { $chosen } line
               *[other] Discard { $chosen } lines
            }
               *[Revert] { $chosen ->
                [one] Revert { $chosen } line
               *[other] Revert { $chosen } lines
            }
            }
repo-core-2377 = Unselect the lines
repo-core-2378 = { $chosen ->
                [one] Revert { $chosen } line
               *[other] Revert { $chosen } lines
            }
repo-core-2379 = Undo only the lines clicked in the working tree, to look over before committing. Revert changes to this file is in the file name's menu.
repo-core-2380 = Revert file
repo-core-2381 = Undo this commit's changes to the file in the working tree, to look over before committing
repo-core-2382 = { $from } → working tree
repo-core-2384 = Line
repo-core-2385 = Lines
repo-core-2386 = { $noun } { $parts }
repo-core-2387 = Wrap long patch lines in any text file
repo-core-2389 = Show whitespace changes
repo-core-2390 = Ignore whitespace changes
repo-core-2392 = Show only the changes
repo-core-2393 = Show the whole file
repo-core-2395 = Side by side
repo-core-2397 = Unified
repo-core-2399 = Next change ({ $NextChange } / { $NextChangeAlt })
repo-core-2401 = Previous change ({ $PreviousChange } / { $PreviousChangeAlt })
repo-core-2404 = Image pixels have no searchable text. Close Find to continue image comparison.
repo-core-2405 = Rendered SVG
repo-core-2406 = SVG source
repo-core-2407 = No changes besides whitespace
repo-core-2408 = No textual changes
repo-core-2414 = Show the diff in place of the graph
repo-core-2415 = Show the diff below the graph
repo-core-2417 = { $first } · { $n } more
repo-core-2418 = Nothing to do here
repo-core-2419 = Drop on a branch or a commit · Esc cancels
repo-core-2422 = Merge { $hash } into { $into }
repo-core-2423 = Rebase { $branch } onto { $hash }
repo-core-2424 = Interactive rebase { $branch } onto { $label }…
repo-core-2425 = Fast-forward { $branch } to { $label }
repo-core-2426 = Cherry-pick { $hash } onto { $onto }
repo-core-2427 = Changes { $into }: a merge commit bringing in { $label }'s changes (git merge)
repo-core-2428 = Changes { $branch }: replays its own commits on top of { $label } (git rebase)
repo-core-2429 = Changes { $branch }: reorder, reword, squash or drop its commits first, then rebase
repo-core-2430 = Moves { $branch } forward to { $label } without checking it out; { $label } stays as it is
repo-core-2431 = Moves { $branch } forward to { $label }: it has nothing { $label } lacks
repo-core-2432 = Copies the commit's changes onto { $onto } as a new commit
repo-core-2433 = Reorder: it comes right after that commit
repo-core-2434 = Reorder: it comes right before that commit
repo-core-2435 = Meld its changes into that commit, keeping both messages
repo-core-2436 = Meld its changes into that commit, keeping that commit's message
repo-core-2437 = Move { $hash } above { $target }
repo-core-2438 = Move { $hash } below { $target }
repo-core-2439 = Squash { $hash } into { $target }
repo-core-2440 = Fixup { $hash } into { $target }
repo-core-2441 = Move
repo-core-2442 = above
repo-core-2443 = below
repo-core-2444 = Squash
repo-core-2445 = into
repo-core-2446 = Fixup
repo-core-2447 = { $verb } { $hash } { $word } { $target }
repo-core-2448 = merges came after the commit, which this would flatten
repo-core-2449 = only commits of the checked-out branch can be moved
repo-core-2450 = it is there already
repo-core-2452 = { $label }: { $self } is still running, try again when it's done
repo-core-2454 = A remerge has a synthetic base: view it in gitcraft
repo-core-2455 = { $path } isn't tracked yet: there is nothing to compare it with
repo-core-2456 = { $path } is conflicted: use Resolve in merge tool
repo-core-2457 = . It runs on { $host }, so it has to be installed there
repo-core-2458 = : the view refreshes when it closes
repo-core-2459 = Showing { $file } in { $label }{ $refreshes }{ $path }
repo-core-2460 = { $path } is already open in a merge tool: finish there first
repo-core-2461 = Waiting for { $label } on { $path }: save and close it to finish{ $path2 }
repo-core-2462 = Resolved { $path } in { $label }
repo-core-2463 = Showing { $file } in{ " " }
repo-core-2465 = Binary file, { $size } bytes
repo-core-2466 = Empty file
repo-core-2470 = Pinned line is outside this file revision
repo-core-2471 = Line { $i }: { $line }
repo-core-2472 = line { $start }
repo-core-2473 = lines { $start }–{ $end }
repo-core-2474 = Every commit up to this version that changed { $which }, followed back through earlier edits and renames
repo-core-2477 = line { $i }, go to commit { $short }
repo-core-2478 = Not committed yet
repo-core-2479 = { $author } · { $now }
            { $summary }
            Click to show the commit
repo-core-2482 = No commits touch these lines yet.
repo-core-2483 = No commits touch this file yet.
repo-core-2485 = { $summary }  ·  as { $path }
repo-core-2487 = Click: what this commit did to the lines (numbered as they were in it)
            Double-click: show the commit in the graph
repo-core-2488 = Click: this commit's changes to the file, or the file as it was
            Double-click: show the commit in the graph
repo-core-2489 = Whole file
repo-core-2490 = Every commit that changed the file, not only these lines
repo-core-2491 = Line { $start }
repo-core-2492 = Lines { $start }–{ $end }
repo-core-2493 = before { $commit }
repo-core-2494 = at { $rev }
repo-core-2495 = as staged
repo-core-2496 = in the working tree
repo-core-2497 = History
repo-core-2498 = Blame
repo-core-2500 = Stopped at the first { $MAX-HITS } matches; type more to narrow the search
repo-core-2501 = Next match; Enter in the find field
repo-core-2502 = Previous match; Shift+Enter in the find field
repo-core-2504 = Find in { $what }
repo-core-2506 = Match case
repo-core-2508 = Match whole words only
repo-core-2509 = Use a regular expression
repo-core-2510 = the diff
repo-core-2511 = the file
repo-core-2512 = the blame
repo-core-2513 = The text to find is too long (more than { $MAX-PATTERN } characters)
repo-core-2516 = Find reading view · rendered text · Close Find to restore the rich layout
repo-core-2517 = Rendered Find is unavailable: this reading exceeds 32768 logical rows or 20 MiB of rendered text. Close Find to use the original view; no partial result count is shown.
repo-core-2518 = Copy current reading row
repo-core-2520 = A { $to-lowercase } is in progress: finish or abort it first
repo-core-2521 = Nothing is staged: stage the changes to fold in first
repo-core-2522 = Merges came after this commit: rewriting it would flatten them
repo-core-2523 = A merge commit can't take the staged changes
repo-core-2524 = Only commits of the checked-out branch can take the staged changes
repo-core-2525 = This clone is shallow: the commits before { $target } aren't here to rebase onto
repo-core-2526 = The ignored { $file } is where a commit to rewrite has a file: the rebase would write over it. Move it away first
repo-core-2527 = { $target } is not on the checked-out branch
repo-core-2528 = the commit could not be read
repo-core-2529 = Commit fixup! for { $hash }
repo-core-2530 = Fold into { $hash }
repo-core-2534 = git { $VERIFY } failed
repo-core-2538 = { $label } (its fixup! commit)
repo-core-2539 = { $name } is folded
repo-core-2540 = Show the changes
repo-core-2541 = Settings → Git → Diffs turns folding off
repo-core-2543 = { $count ->
            [one] { $count } generated or folded file
           *[other] { $count } generated or folded files
        }
repo-core-2544 = Fold them again
repo-core-2545 = Show them (Settings → Git → Diffs turns folding off)
repo-core-2546 = lockfile
repo-core-2547 = lockfiles
repo-core-2548 = generated file
repo-core-2549 = generated files
repo-core-2550 = minified file
repo-core-2551 = minified files
repo-core-2552 = file marked -diff
repo-core-2553 = files marked -diff
repo-core-2554 = folded file
repo-core-2555 = folded files
repo-core-2577 = generated
repo-core-2578 = marked -diff
repo-core-2579 = minified
repo-core-2580 = matches a pattern of Settings → Git → Diffs
repo-core-2582 = Open Gerrit change
repo-core-2585 = GRAPH · { $to-uppercase }
repo-core-2586 = GRAPH
repo-core-2587 = Order
repo-core-2588 = First parent only
repo-core-2589 = Each merge shows as one commit, without the commits it brought in; search finds only what is shown (git log --first-parent)
repo-core-2591 = tag { $label }
repo-core-2592 = detached HEAD
repo-core-2593 = pull request branch { $label }
repo-core-2594 = branch { $label }, checked out
repo-core-2595 = remote branch { $git-ref }
repo-core-2596 = branch { $label }
repo-core-2598 = HEAD (detached)
repo-core-2599 = { $label } — pull request branch (not fetched as a branch)
repo-core-2600 = local, checked out
repo-core-2601 = local
repo-core-2609 = Resize the { $bar } column
repo-core-2612 = BRANCH / TAG
repo-core-2613 = DESCRIPTION
repo-core-2614 = AUTHOR
repo-core-2615 = WHEN
repo-core-2616 = DATE
repo-core-2617 = When
repo-core-2618 = Date
repo-core-2619 = Fit visible branch / tag pills
repo-core-2620 = Compact graph column
repo-core-2621 = Fit graph column to its lanes
repo-core-2622 = The graph column already fits its lanes
repo-core-2623 = As double-clicking its border does
repo-core-2624 = Reset columns
repo-core-2625 = Every column back to its first width
repo-core-2627 = Create patch from commit…
repo-core-2628 = A merge commit makes no patch
repo-core-2629 = Save it as a file for git am (git format-patch)
repo-core-2630 = Apply patch…
repo-core-2631 = branch { $name }
repo-core-2632 = Check out { $branch }
repo-core-2634 = Fast-forward { $current } to { $branch }
repo-core-2636 = Merge { $branch } into { $current }
repo-core-2637 = Rebase { $current } onto { $branch }
repo-core-2638 = Interactive rebase { $current } onto { $branch }…
repo-core-2639 = Branch { $name }
repo-core-2641 = Create branch here…
repo-core-2642 = Create tag here…
repo-core-2643 = The next version, notes from the commits since the last tag, a tag, the push and the forge's release, shown before it runs
repo-core-2644 = Create worktree from this commit…
repo-core-2646 = A second working folder on a new branch starting here
repo-core-2647 = Check out this commit (detached)
repo-core-2648 = Compare with { $other }
repo-core-2649 = Also Ctrl+click on a commit
repo-core-2650 = Compare with the working tree
repo-core-2651 = working tree
repo-core-2652 = Interactive rebase { $target } onto this commit…
repo-core-2653 = Reorder, reword, squash or drop the commits after this one
repo-core-2654 = Cherry-pick onto current branch
repo-core-2655 = Revert commit
repo-core-2656 = Reset { $target } to here
repo-core-2657 = Drops the commits after this one, keeps uncommitted changes. It stops, changing nothing, if a file with changes is one those commits touched.
repo-core-2659 = { $label } reset
repo-core-2660 = Move { $target } to { $short } ({ $summary })? { $explain }
repo-core-2661 = Leaving { $target }
repo-core-2662 = Leaving { $target }, changes kept
repo-core-2663 = Copy commit hash
repo-core-2664 = Copy message
repo-core-2665 = Copy branch name ({ $name })
repo-core-2668 = Loading full commit message…
repo-core-2669 = Minimap analysis cancelled
repo-core-2670 = Shape shows the first { $LANES } of { $width } lanes; all rows and HEAD, tag and search markers remain represented.
repo-core-2671 = Shape shows all { $max } lanes.
repo-core-2673 = Loaded history minimap. { $coverage } Drag to scroll; arrow and Page Up/Down keys move the viewport. Blue HEAD, green tags, yellow search hits.
repo-core-2674 = History · 24 lanes
repo-core-2675 = Older unloaded ↓
repo-core-2676 = History ends
repo-core-2677 = { $rows } projected rows of loaded history. { $coverage } Drag to scroll; focused: arrows or Page Up/Down. Blue: HEAD · green: tags · yellow: search hits · grey: collapsed context. { $loaded }
repo-core-2678 = Older history is not loaded; scrolling to the boundary loads another page.
repo-core-2679 = All available history is loaded.
repo-core-2680 = Order history, or show first parents only
repo-core-2681 = Order history: { $label }
repo-core-2683 = Pull request tip; double-click its entry in the left pane to open details
repo-core-2684 = Double-click a pill to check it out, or drag it onto a branch or a commit
repo-core-2685 = Double-click a pill to check it out; drag the row to cherry-pick or move the commit
repo-core-2686 = { $n }
            Tip commit by { $author } · { $date }
            { $drag }
repo-core-2690 = pull request tip
repo-core-2691 = checked out
repo-core-2692 = double-click to pull
repo-core-2693 = double-click to check out
repo-core-2699 = detached
repo-core-2701 = . Newly received commit
repo-core-2704 = Open Gerrit change { $id }
repo-core-2707 = Uncommitted changes
repo-core-2710 = The first bad commit, as the bisect found
repo-core-2711 = Marked { $word } in the bisect
repo-core-2712 = Next (Enter)
repo-core-2713 = Previous (Shift+Enter)
repo-core-2715 = Search code changes, or author:, path:…
repo-core-2716 = Search messages, authors, hashes, or author:, path:…
repo-core-2718 = Messages
repo-core-2719 = Search commit messages, authors and hashes
repo-core-2720 = Code
repo-core-2721 = Search the commits that added or removed this text in a file (git log -S)
repo-core-2722 = searching…
repo-core-2723 = no results
repo-core-2724 = { $current } of { $n }
repo-core-2725 = Operators, combined with each other and with text:
            { "" }
repo-core-2726 = { "" }
            { "" }
            Quote values with spaces: author:"Jane Doe".
            Dates: { $DATE-FORMATS }.
repo-core-2729 = leave merge commits out
repo-core-2730 = Reading { $what }…
repo-core-2731 = Couldn't read the { $what }: { $e }
repo-core-2732 = authors
repo-core-2733 = committers
repo-core-2735 = { $commits ->
                [one] { $email } · { $commits } commit
               *[other] { $email } · { $commits } commits
            }
repo-core-2736 = No { $what } match
repo-core-2740 = No file or folder matches
repo-core-2747 = Narration cancelled
repo-core-2748 = No loaded commit in that direction; older or hidden history may continue beyond this view
repo-core-2749 = Topology analysis is preparing
repo-core-2750 = divergence
repo-core-2751 = root
repo-core-2752 = { $count ->
        [one] { $children ->
        [one] { $shape }; { $count } parent, { $children } loaded child
       *[other] { $shape }; { $count } parent, { $children } loaded children
    }
       *[other] { $children ->
        [one] { $shape }; { $count } parents, { $children } loaded child
       *[other] { $shape }; { $count } parents, { $children } loaded children
    }
    }
repo-core-2753 = ; parents continue beyond loaded history
repo-core-2754 = ; branch { $name }
repo-core-2755 = ; upstream removed
repo-core-2756 = , { $ahead } ahead and { $behind } behind { $upstream }
repo-core-2757 = Topology analysis cancelled
repo-core-2758 = Selected history: { $ahead } ahead and { $behind } behind { $name } within { $count } loaded commits
repo-core-2759 = ; shared ancestor { $hash } (author date { $date }). Git does not record a branch creation date
repo-core-2760 = ; no shared ancestor in loaded history
repo-core-2761 = ; selected ancestry introduced by { $merges } observed merges on the first-parent line of { $release }
repo-core-2762 = { " " }(partial: release ancestry is not fully loaded)
repo-core-2763 = ; comparison is partial: unloaded ancestors may change counts
repo-core-2764 = { $GraphNext } first parent · { $GraphPrevious } child · { $GraphRight } next descendant merge · { $GraphLeft } branch tip
repo-core-2765 = Child
repo-core-2766 = Choose a parent or child…
repo-core-2767 = { $kind }: { $hash } { $summary }
repo-core-2768 = Graph sounds
repo-core-2769 = Audio unavailable; navigation and descriptions remain available
repo-core-2770 = Retry audio
repo-core-2771 = Frame cancelled
repo-core-2772 = Replay tree expanded work budget exceeded
repo-core-2773 = Replay tree expanded allocation/entry budget exceeded
repo-core-2774 = Watch the repository grow
repo-core-2776 = Reading pinned first-parent history and baseline tree…
repo-core-2777 = { $count } first-parent commits of HEAD · { $history }{ $else }
repo-core-2778 = latest 1,000 commits (partial history)
repo-core-2779 = all available first-parent history
repo-core-2780 = { " " }· shallow boundary
repo-core-2781 = The first frame is a full pinned tree. Later frames apply each commit, including merges against their first parent. Deleted files remain crossed out for their deletion frame. Branch histories are not interleaved.
repo-core-2782 = No commits yet.
repo-core-2783 = Reduced motion is enabled. Scrub or step through static frames.
repo-core-2784 = Pause growth
repo-core-2785 = Play growth
repo-core-2786 = Previous commit
repo-core-2787 = Next commit
repo-core-2788 = commits / second
repo-core-2789 = History frame
repo-core-2790 = Preparing frame { $cursor }…
repo-core-2791 = Open this commit
repo-core-2792 = { $count ->
        [one] { $count2 ->
        [one] { $count } tree entry · { $count2 } path changed in this frame
       *[other] { $count } tree entry · { $count2 } paths changed in this frame
    }
       *[other] { $count2 ->
        [one] { $count } tree entries · { $count2 } path changed in this frame
       *[other] { $count } tree entries · { $count2 } paths changed in this frame
    }
    }
repo-core-2793 = { "" }
            { $name } worked here at this commit
repo-core-2797 = { " " }… { $count } more characters
repo-core-2798 = History worker stopped
repo-core-2801 = Pushes once without running the pre-push hook (git push --no-verify)
repo-core-2803 = This endpoint is outside the failed operation's verified routes. Inspect its account separately in HTTPS sign-ins, then run the operation again; no account rejection or retry was authorized.
repo-core-2804 = The operation used a previous connection. Run it again with the current profile before signing in.
repo-core-2805 = LFS or a secondary endpoint needs its own explicitly selected account. The HTTPS sign-ins page shows the reported endpoint; no account rejection or automatic retry has been authorized.
repo-core-2806 = The failed operation has no verified destination snapshot. Run it again before signing in; LFS or hook sign-ins require their own explicitly selected account.
repo-core-2807 = Git did not report an HTTP(S) sign-in endpoint. Inspect HTTPS sign-ins in Settings, then run the operation again.
repo-core-2808 = Sign-in rejection for the previous connection { $failed }. Current sign-in was not changed.
repo-core-2809 = finished
repo-core-2810 = Stored HTTPS sign-in rejected. Choose the correct account; profile-selected forge accounts can be changed in Settings → Profile.
repo-core-2811 = Could not forget the stored sign-in: { $e }
repo-core-2812 = These two commits fail together; leaving either out passes or can't be tested (it doesn't apply without the other):
repo-core-2813 = These { $n } commits fail together; leaving any one out passes or can't be tested (it doesn't apply without the others):
repo-core-2814 = Out of tests ({ $budget }): these { $count } commits fail together; fewer of them may too
repo-core-2815 = This commit fails on its own, applied where the two diverged:
repo-core-2816 = These two commits fail together; either alone passes:
repo-core-2817 = These { $n } commits fail together; leaving out any one of them passes:
repo-core-2818 = { $a } and { $b } pass together: there is nothing to find.
repo-core-2819 = { $a } fails on its own: it isn't the two together. Test its commits instead.
repo-core-2820 = { $b } fails on its own: it isn't the two together. Test its commits instead.
repo-core-2821 = The commits of { $a } and { $b } don't apply together: rehearse the merge to see the conflicts.
repo-core-2822 = The two together don't give the same result twice: the test is flaky.
repo-core-2823 = 1 test run
repo-core-2824 = { $n ->
            [one] { $n } test run
           *[other] { $n } tests run
        }
repo-core-2825 = { " " }· 1 result didn't repeat and was set aside
repo-core-2826 = { " " }· { $n } results didn't repeat and were set aside
repo-core-2827 = pass
repo-core-2828 = fail
repo-core-2829 = can't tell (they don't apply, or it didn't repeat)
repo-core-2831 = Test { $tests } of at most { $budget }: { $size ->
            [one] 1 commit { $probe ->
            [pass] passes
            [fail] fails
           *[unresolved] cannot be tested
        }
           *[other] { $size } commits { $probe ->
            [pass] pass
            [fail] fail
           *[unresolved] cannot be tested
        }
        }
repo-core-2832 = Find the interaction: { $a } and { $b }
repo-core-2835 = Stopped before it found anything.
repo-core-2836 = The search failed: { $e }
repo-core-2837 = Select it in the graph
repo-core-2838 = on { $side }
repo-core-2839 = Testing the two together…
repo-core-2840 = Each test applies some of the commits where the two diverged and runs `{ $command }`; a failure is run { $reruns } more time(s) to be sure.
repo-core-2844 = Update submodule { $path }
repo-core-2847 = Repository sidebar
repo-core-2848 = Repository graph or diff
repo-core-2849 = Commit details and changed files
repo-core-2850 = File { $path }
repo-core-2851 = Focus the corresponding file list or working-tree diff first
repo-core-2852 = Wait for a successful repository read before using this selection
repo-core-2853 = Only non-conflicted working-tree files can be staged or unstaged
repo-core-2854 = Use the current ordinary patch with whitespace changes visible to change a hunk
repo-core-2855 = Stage/discard needs an unstaged hunk; unstage needs a staged hunk
repo-core-2856 = This hunk has no editable changed lines
repo-core-2857 = No history loaded
repo-core-2858 = No rebase editor is open
repo-core-2859 = The rebase is being prepared
repo-core-2860 = { $op } is still running
repo-core-2861 = This repository has no remote
repo-core-2862 = Nothing to undo
repo-core-2863 = Nothing to redo
repo-core-2864 = Select a loaded commit with this value first
repo-core-2865 = No previous view
repo-core-2866 = No next view
repo-core-2867 = No changes to stash
repo-core-2868 = Make a first commit to branch from
repo-core-2869 = Nothing to commit
repo-core-2870 = This command is available in its owning view
repo-core-2878 = Opening { $name }
repo-core-2879 = Reading branches and history…
repo-core-2891 = Hide the { $name } panel
repo-core-2892 = Show the { $name } panel
repo-core-2893 = Could not read the worktrees root: { $e }
repo-core-2894 = Reading older history failed
repo-core-2896 = cancelled
repo-core-2897 = no details
repo-core-2898 = Internal error: { $said }
repo-core-2899 = Background fetch
repo-core-2900 = Commit { $hash } is not in the loaded history
repo-core-2901 = the graph without the diff
repo-core-2902 = the graph at the changes
repo-core-2903 = the graph at { $hash }
repo-core-2904 = the graph
repo-core-2905 = the history
repo-core-2906 = the contents
repo-core-2907 = { " " }at { $r }
repo-core-2908 = { $what } of { $path }{ $at }
repo-core-2909 = pull request #{ $number }
repo-core-2910 = the builds of { $sha }
repo-core-2911 = { $name }, resolution of { $hash }
repo-core-2912 = { $name } in { $hash }
repo-core-2913 = the unstaged changes of { $name }
repo-core-2914 = the staged changes of { $name }
repo-core-2915 = the conflict in { $name }
repo-core-2916 = the new file { $name }
repo-core-2917 = { $name } from { $from } to { $to }
repo-core-2918 = { $name } from { $from } to the working tree
repo-core-2919 = { $path } has no unstaged changes any more
repo-core-2920 = { $path } has no staged changes any more
repo-core-2921 = { $path } is no longer conflicted
repo-core-2922 = { $path } is no longer a new file
repo-core-2923 = Finish or close the { $editor } first
repo-core-2924 = conflict editor
repo-core-2925 = interactive rebase
repo-core-2926 = commit message being edited
repo-core-2927 = Commit { $hash } is no longer in the graph
repo-core-2929 = Back
repo-core-2930 = Forward
repo-core-2931 = { $name } to { $graph-shown } ({ $keys })
repo-core-2932 = Nothing to go { $to-lowercase } to ({ $keys })
repo-core-2933 = { $why }: turn it off to fetch, pull or push
repo-core-2934 = { $label } · last fetched { $d } ago
repo-core-2935 = { $label } · never fetched
repo-core-2938 = The background fetch, avatars, update checks and downloads and the polling of builds and pull requests are paused.
repo-core-2939 = { $why }. { $what } Fetch, Pull, Push and the like don't run.
            Click to go online.
repo-core-2940 = { $why }. { $what } What you ask for still runs.
            They start again by themselves once there is a connection.
repo-core-2941 = { $why }. { $what } What you ask for still runs.
            Settings → Git can let them run on a metered connection too.
repo-core-2942 = under a minute
repo-core-2943 = { $s } min
repo-core-2944 = { $s } h
repo-core-2945 = { $value } days
repo-core-2947 = { $label }: not started, { $busy } is still running
repo-core-2948 = { $label }: not started. { $why }
repo-core-2949 = Push after { $label }
repo-core-2950 = Wait until { $busy } finishes before switching sign-ins
repo-core-2951 = This sign-in has no verified operation route. Run the operation again before switching accounts.
repo-core-2952 = The sign-in destination or account context changed. Run the operation again before signing in.
repo-core-2953 = This sign-in has no verified destination snapshot. Run the operation again.
repo-core-2954 = Checking the sign-in destination…
repo-core-2955 = Sign-in was discarded because the operation or connection changed.
repo-core-2956 = Could not preserve the sign-in route. Run the operation again.
repo-core-2957 = { $label }: run it again once { $busy } is done
repo-core-2959 = { $label }: done, but { $note }
repo-core-2962 = { $label }: sign-in needed
repo-core-2963 = The reviewed push's sign-in proof is unavailable. Reopen push preflight before signing in.
repo-core-2965 = { $name } · { $label }: done
repo-core-2966 = { $name } · { $label } failed: { $e }
repo-core-2967 = Could not verify the network operation's destinations; command not started: { $error }
repo-core-2969 = Invalid reviewed push execution evidence
repo-core-2971 = Check out a branch…
repo-core-2972 = Merge into { $current }…
repo-core-2973 = Rebase { $current } onto…
repo-core-2974 = Interactive rebase { $current } onto…
repo-core-2975 = Changes a branch adds to { $current }…
repo-core-2976 = Compare { $current } with…
repo-core-2977 = Find the interaction of { $current } with…
repo-core-2979 = Push another branch…
repo-core-2981 = to { $to }
repo-core-2982 = Cherry-pick { $at }
repo-core-2984 = Revert { $at }
repo-core-2986 = Reset { $target } to here…
repo-core-2988 = Interactive rebase { $target } onto { $at }…
repo-core-2990 = Create branch at { $at }…
repo-core-2992 = Create tag at { $at }…
repo-core-2994 = Create patch from { $at }…
repo-core-2996 = View
repo-core-2998 = File history of
repo-core-2999 = { $label } { $path }
repo-core-3001 = Open { $path } in editor
repo-core-3003 = Open { $path }
repo-core-3005 = Delete { $path }…
repo-core-3006 = Discard changes to { $path }…
repo-core-3007 = Stage { $path }
repo-core-3010 = Unstage { $path }
repo-core-3012 = Save patch
repo-core-3013 = Patch
repo-core-3016 = Patch saved to { $file }
repo-core-3017 = Create patch failed: { $e }
repo-core-3018 = Apply patch
repo-core-3021 = All files
repo-core-3022 = { $name } is busy: { $busy }
repo-core-3023 = { $name } is still applying a patch
repo-core-3024 = could not read { $file }: { $e }
repo-core-3025 = Apply patch: { $why }
repo-core-3026 = Apply { $files }
repo-core-3028 = internal error while copying it there
repo-core-3029 = wait until { $self } is done
repo-core-3041 = { $noun } in its own window
repo-core-3042 = Show that window
repo-core-3043 = Show the { $noun } window
repo-core-3046 = some may be required
repo-core-3047 = none is required
repo-core-3048 = { $failed } check(s) failed and { $running } still running, but { $required }.
repo-core-3049 = Couldn't read what can be done here. Load it again.
repo-core-3050 = Merge commit
repo-core-3051 = Rebase
repo-core-3052 = { $name } deletes { $head-branch } afterwards: this repository is set to.
repo-core-3053 = The forge
repo-core-3054 = Delete { $head-branch } afterwards
repo-core-3055 = The repository's rules aren't met, but you may merge past them.
repo-core-3056 = Complete on { $name }
repo-core-3058 = Merge #{ $number } into { $base-branch }? Everyone sees this at once, and it can't be undone from here.
repo-core-3059 = Yes, merge
repo-core-3060 = Merge pull request
repo-core-3062 = PRACTICE · { $title }
repo-core-3063 = Local throwaway repository
repo-core-3064 = Check goal
repo-core-3065 = Hint
repo-core-3067 = Checking the repository…
repo-core-3068 = Goal complete. You can keep experimenting or start another scenario from Practice.
repo-core-3069 = Not yet. Follow the goal above, then check again.
repo-core-3070 = Could not check the goal: { $error }
repo-core-3071 = Worktree or profile changed while reading; read evidence again in the selected context
repo-core-3072 = Local evidence only. Nothing is uploaded, executed or exported through MCP. Entire checkpoints already in Git and the recorded commit message are read; local Claude/Codex discovery is opt in. Multiple time/path matches remain separate candidates.
repo-core-3073 = Include local Claude Code and Codex transcripts
repo-core-3074 = Physical transcript roots, one per line (up to eight). Only bounded JSONL files are read; config files are excluded by the JSONL-only allowlist.
repo-core-3075 = Match records within hours before commit:
repo-core-3076 = and 10 minutes after
repo-core-3077 = Read limits: 4 MiB per file, 32 MiB total, 200 newest local files/root, 2,000 steps/transcript. Unavailable, truncated or ambiguous evidence is reported explicitly.
repo-core-3078 = Read local prompt/session evidence
repo-core-3079 = Reading bounded local evidence…
repo-core-3080 = Read evidence for this selected commit. Previous commit evidence is not shown here.
repo-core-3081 = Hunk { $path }:{ $range }–{ $range2 } ({ $new })
repo-core-3082 = No unique recorded edit snippet matches these commit lines. A session/time match alone is not hunk attribution.
repo-core-3083 = { $count ->
        [one] { $count } recorded content match; inspect each candidate step
       *[other] { $count } recorded content matches; inspect each candidate step
    }
repo-core-3084 = Session { $i }, step { $j }
repo-core-3086 = Step { $j } · { $kind }{ $t }
repo-core-3087 = { " " }· UTC epoch { $t }
repo-core-3088 = No unique exact old/new content match in this commit; no line attribution asserted.
repo-core-3089 = Open commit diff · parent lines { $start }–{ $end }
repo-core-3090 = Open commit lines { $start }–{ $end }
repo-core-3091 = Recorded before:
repo-core-3092 = Recorded after:
repo-core-3093 = Pull (fast-forward if possible)
repo-core-3094 = Pull (fast-forward only)
repo-core-3095 = Pull (rebase)
repo-core-3099 = Fast-forward the current branch, or merge when it has diverged
repo-core-3100 = Fast-forward the current branch
repo-core-3101 = Rebase the current branch onto its upstream
repo-core-3102 = { $Pull }
            git { $self }
            Right-click or the arrow to choose
repo-core-3104 = Fetch all
repo-core-3105 = The preflight worker stopped before completing its checks. No push was run.
repo-core-3106 = Push preflight exceeded its two-minute budget. Fetch missing history or split this push, then recheck; nothing was pushed.
repo-core-3107 = The profile or its identity policy changed. Reopen push preflight.
repo-core-3108 = Wait for the current operation, then recheck the push preview.
repo-core-3109 = The operation timeline changed; reopen push from its original action
repo-core-3110 = Push reviewed history
repo-core-3111 = Rewrite the listed outgoing author mismatches as { $expected-name } <{ $expected-email }>. This can change attribution on other contributors' commits; review the checklist first.
repo-core-3113 = Recreate the outgoing commits with signatures using your configured signing key. Signing may ask you to unlock that key.
repo-core-3115 = Remove every listed likely-secret file from every outgoing commit, and replace entire flagged commit messages. The final working files change too. Review the file list first; recreate safe versions after the repair.
repo-core-3116 = Convert all outgoing versions of the listed large regular files to local Git LFS objects and pointers, and add exact path attributes. No LFS data is uploaded or downloaded here. git-lfs must be installed.
repo-core-3117 = Merge the fetched target commit into this branch, keeping remote history. This may leave conflicts for you to resolve or abort.
repo-core-3118 = Fix outgoing history
repo-core-3119 = { $text }
            { "" }
            Requires this branch checked out with a clean working tree. Ctrl+Z restores a completed repair; a later push is a separate explicit action.
repo-core-3120 = Apply repair
repo-core-3122 = Check out the previewed branch before editing messages, then reopen preflight.
repo-core-3123 = Could not check the branch
repo-core-3124 = Check out the previewed branch before editing its messages.
repo-core-3127 = fetched target / root
repo-core-3132 = Push preflight
repo-core-3134 = Delete remote ref
repo-core-3135 = Delete { $destination } only while the server tip is { $super }. No outgoing objects or LFS payloads are sent.
repo-core-3136 = { $After } reviewed push: { $step }
repo-core-3137 = { $count ->
        [one] { $count } outgoing commit ({ $count2 } with embedded signatures), pinned to { $head }. Author policy: { $expected-name } <{ $expected-email }>.
       *[other] { $count } outgoing commits ({ $count2 } with embedded signatures), pinned to { $head }. Author policy: { $expected-name } <{ $expected-email }>.
    }
repo-core-3138 = Skip original pre-push hook for this push (as requested). The reviewed LFS upload guard still runs.
repo-core-3139 = Selected account for { $host }: { $account } (profile metadata; server identity not queried).
repo-core-3140 = Checks use local/fetched history only. Secret detection is heuristic; it does not download or inspect external LFS contents, verify signature trust, or replace a dedicated secret scanner.
repo-core-3141 = { $count ->
        [one] { $count } LFS object will be uploaded as part of the confirmed push to { $plan }; its external content was not scanned. Endpoint settings use committed .lfsconfig plus inspected Git configuration, ignoring later working-tree edits.
       *[other] { $count } LFS objects will be uploaded as part of the confirmed push to { $plan }; their external content was not scanned. Endpoint settings use committed .lfsconfig plus inspected Git configuration, ignoring later working-tree edits.
    }
repo-core-3142 = Repairs require the underlying branch checked out. The push itself still sends the inspected object.
repo-core-3143 = Check out for repairs
repo-core-3144 = Checkpoint / transcript publication
repo-core-3145 = Author matches this profile's host policy
repo-core-3146 = Amend authors…
repo-core-3147 = Profile signing policy
repo-core-3148 = Sign outgoing commits…
repo-core-3149 = No leftover fixup/WIP subjects
repo-core-3151 = Autosquash / edit messages…
repo-core-3152 = Likely secrets in outgoing objects/messages
repo-core-3153 = Remove affected history…
repo-core-3154 = Large Git blobs (over 5 MiB)
repo-core-3155 = Move to LFS…
repo-core-3156 = Remote commits a force push would discard
repo-core-3157 = Keep remote commits…
repo-core-3160 = Review
repo-core-3161 = clear
repo-core-3162 = { $count ->
            [one] { $count } finding
           *[other] { $count } findings
        }
repo-core-3163 = More findings exist; repair and recheck before pushing.
repo-core-3165 = Rewrite the target with an explicit force-with-lease
repo-core-3166 = No fetched target tip is available: force push is disabled. Fetch first to inspect dropped remote commits.
repo-core-3167 = I reviewed the exact target and findings, including heuristic limitations
repo-core-3168 = Delete inspected remote ref
repo-core-3169 = Push with inspected lease
repo-core-3170 = Push inspected commit
repo-core-3171 = Recheck
repo-core-3172 = Edit commit message
repo-core-3173 = Drop commit
repo-core-3174 = Move commit up
repo-core-3175 = Move commit down
repo-core-3176 = Edit the message in the commit's details
repo-core-3177 = Remove the commit and its changes from the branch
repo-core-3178 = Swap it with the commit after it
repo-core-3179 = Swap it with the commit before it
repo-core-3181 = Only commits of the checked-out branch can be edited here
repo-core-3182 = Merge commits can't be rewritten here
repo-core-3183 = The first commit has no parent to rebase from
repo-core-3184 = It is the latest commit already
repo-core-3185 = The commit below it isn't loaded
repo-core-3186 = The commit below it is a merge
repo-core-3187 = The commit below it is the first one, with nothing to rebase onto
repo-core-3188 = only commits of the checked-out branch can be edited
repo-core-3189 = the message is edited in the commit's details
repo-core-3190 = it is the latest commit already
repo-core-3191 = there is no commit below it to swap with
repo-core-3192 = A { $to-lowercase } is in progress
repo-core-3196 = Remove { $short } ({ $summary }) and its changes from { $branch }? The commits after it are replayed on its parent; if one conflicts, the rebase stops for you to resolve it.
repo-core-3197 = Drop
repo-core-3198 = Leaving { $branch }
repo-core-3199 = { $label } { $hash }: { $busy } is still running, try again when it's done
repo-core-3200 = Pick
repo-core-3201 = Reword
repo-core-3202 = Keep the commit as it is
repo-core-3203 = Keep the changes, edit the message
repo-core-3204 = Meld into the commit below, keeping both messages
repo-core-3205 = Meld into the commit below, dropping this message
repo-core-3206 = Remove the commit and its changes
repo-core-3207 = Stop after this commit to change it, then Continue rebase
repo-core-3208 = The checked-out branch changed since this editor opened. Cancel and start again.
repo-core-3209 = The branch has moved since these commits were read: rebasing would drop the new ones. Cancel and start again.
repo-core-3210 = These commits include merges, which this editor would flatten. Use git rebase -i --rebase-merges in the terminal.
repo-core-3211 = Keep at least one commit (or reset the branch instead).
repo-core-3212 = { $label } { $hash } has no commit below it to meld into.
repo-core-3213 = A reworded commit needs a message.
repo-core-3214 = could not prepare the rebase: { $e }
repo-core-3235 = Drag a row by its handle, or { $RebaseMoveUp } / { $RebaseMoveDown }, to move it. { $RebasePrevious } / { $RebaseNext } select a row. { $steps }
repo-core-3236 = Interactive rebase · { $branch } onto { $onto-label }
repo-core-3237 = Cancel (Esc)
repo-core-3238 = Recheck rebase weather
repo-core-3240 = Update dependent branches
repo-core-3241 = Move the branches on these commits along with them (git rebase --update-refs, git 2.38+): the lower layers of a stack rebased from its top. Branches checked out in other worktrees are left where they are; Restack… in STACKS handles those. Undo puts every branch back.
repo-core-3242 = Start rebase
repo-core-3243 = { $n ->
                [one] { $n } commit , newest on top · { $Undo } undoes the rebase
               *[other] { $n } commits , newest on top · { $Undo } undoes the rebase
            }
repo-core-3246 = Move up, later
repo-core-3247 = Move down, earlier
repo-core-3249 = Rebase commit { $hash }, { $summary }, { $author }, { $label }
repo-core-3250 = ↓ into the commit below
repo-core-3251 = stops here so you can change it
repo-core-3255 = Forecast time budget exceeded; this position was not checked. Recheck rebase weather to try again.
repo-core-3256 = Not checked: an earlier commit conflicts. Resolve it before later positions can be predicted.
repo-core-3257 = Forecast unavailable (requires Git with --attr-source and merge-tree --write-tree --merge-base): { $e }
repo-core-3273 = Disabled in Settings
repo-core-3275 = Binary conflict
repo-core-3276 = Structural or binary conflict; Mergiraf cannot check this forecast
repo-core-3277 = Forecast cancelled or timed out
repo-core-3278 = Mergiraf solves the text conflict (not applied)
repo-core-3279 = Mergiraf leaves conflicts
repo-core-3280 = Mergiraf: { $e }
repo-core-3281 = Git returned no forecast tree
repo-core-3282 = Dropped: not replayed
repo-core-3284 = Applies cleanly with Git defaults and simulated-tree attributes. Source merge settings, hooks, custom drivers and global/info attributes are excluded.
repo-core-3286 = Conflicts here (simulated-tree attributes):
            { $n }
repo-core-3288 = Becomes empty here. Git may stop for you to skip or keep this commit.
repo-core-3289 = Checking rebase weather…
repo-core-3292 = Unknown repository object format
repo-core-3294 = Could not prepare forecast context
repo-core-3295 = Invalid forecast temporary path
repo-core-3309 = Revert { $path } as changed in { $hash }
repo-core-3310 = Revert
repo-core-3311 = Revert a hunk of { $path } from { $hash }
repo-core-3312 = { $n ->
                [one] Revert { $n } line of { $path } from { $hash }
               *[other] Revert { $n } lines of { $path } from { $hash }
            }
repo-core-3313 = { $label }: not done, { $busy } was still running. Try again once it's done.
repo-core-3316 = { $label }: done. The change is in the working tree, to look over before committing (Ctrl+Z undoes it).
repo-core-3317 = { $label }: the file has changed since, so the revert was merged in with a 3-way merge and staged. Look it over before committing; Ctrl+Z can't undo this one (unstage the file and discard its changes instead).
repo-core-3318 = { $label }: conflicts with changes made since, marked in { $files }. Settle them under Conflicted, or discard the file's changes to drop the revert.
repo-core-3321 = Edit the message of { $hash }
repo-core-3322 = Edit message: { $e }
repo-core-3323 = Edit message: { $self } is still running, try again when it's done
repo-core-3324 = Nothing to rebase: { $label } already contains every commit of this branch
repo-core-3325 = Interactive rebase: { $e }
repo-core-3328 = Invalid rebase target
repo-core-3329 = Rebase preview: { $e }
repo-core-3330 = The branch changed while preparing the rebase preview; reopen it.
repo-core-3331 = Replay the branch's unique commits onto the pinned target. Conflicts stop for resolution. Dirty tracked changes prevent this rebase from starting.
repo-core-3332 = Commits rewritten on { $branch }
repo-core-3337 = Interactive rebase
repo-core-3338 = Replay this todo list? Dropped commits leave the branch; kept commits may receive new IDs. Git autostashes and restores tracked uncommitted changes; restoring that stash can conflict.
repo-core-3339 = Commits rewritten or dropped on { $branch }
repo-core-3340 = The approved rebase branch, HEAD or todo changed. Reopen its preview.
repo-core-3341 = The approved rebase context changed. Reopen its preview.
repo-core-3342 = Interactive rebase onto { $onto-label }
repo-core-3343 = The approved rebase branch, HEAD or todo changed while preparing its command. Reopen its preview.
repo-core-3344 = The approved rebase has no retained branch identity; reopen its preview.
repo-core-3345 = { $label }: something else is running, try again when it's done
repo-core-3346 = The prepared rebase does not match its approved HEAD.
repo-core-3347 = { $label }: not started, { $MAX-WAITING } operations already wait for { $busy }
repo-core-3348 = { " " }({ $names } not run)
repo-core-3355 = Hard reset
repo-core-3356 = Reset
repo-core-3359 = Checked out in another worktree
repo-core-3360 = Can't delete the checked-out branch
repo-core-3361 = Delete branch
repo-core-3362 = Delete the local branch “{ $name }”?
repo-core-3363 = Force, even if it isn't merged (-D)
repo-core-3364 = Delete locally and on { $remote }…
repo-core-3365 = Also delete { $upstream } from { $remote }
repo-core-3367 = Delete branch locally and on the remote
repo-core-3368 = Delete the local branch “{ $name }” and “{ $name2 }” on { $remote }? Deleting it on the remote changes it for everyone.
repo-core-3369 = Delete both
repo-core-3375 = LOCAL
repo-core-3376 = New branch at HEAD…
repo-core-3379 = Not tracking a remote branch
repo-core-3380 = { $upstream } was deleted on the remote
repo-core-3381 = Tracks { $upstream }
repo-core-3382 = { "" }
            CI: { $state }
repo-core-3383 = { "" }
            Pinned to top
repo-core-3384 = { $name }
            { $tip }
            Checked out in worktree { $wt }
            Double-click to open it, or drag onto a branch or a commit
repo-core-3385 = { $name }
            { $tip }
            Double-click to check out, or drag onto a branch or a commit
repo-core-3386 = Tags it, merges it back into the main branch (and the development one), deletes it and pushes it all, after showing what it will do
repo-core-3387 = Changes { $name } adds to { $name2 }
repo-core-3388 = What it changed since the two diverged, as a pull request shows it
repo-core-3389 = Rename…
repo-core-3390 = Copy name
repo-core-3391 = Show reflog
repo-core-3392 = Where the branch has been: each commit it pointed at, and what moved it
repo-core-3393 = How much agents wrote
repo-core-3394 = Of the lines this branch added since it left the main branch: how many agents wrote (as the commits record it), and how many of those people edited since
repo-core-3395 = Review this branch
repo-core-3396 = No base branch is available for this review
repo-core-3397 = There is no other branch to review it against
repo-core-3398 = Have your AI tool review what { $name } adds to { $base }
repo-core-3399 = Pick an AI tool first (Settings → AI, or the sparkles button of the commit box)
repo-core-3400 = Unpin
repo-core-3401 = Pin to top
repo-core-3403 = Pinned branches stay at the top of Local
repo-core-3404 = Open its worktree
repo-core-3405 = Open its worktree in a tab
repo-core-3406 = Check out
repo-core-3407 = Check out in a new worktree…
repo-core-3408 = Push to { $to }
repo-core-3409 = Set upstream…
repo-core-3410 = Open { $noun } #{ $number }
repo-core-3412 = Create { $noun }…
repo-core-3413 = Force push via push route (with lease)…
repo-core-3415 = Interactive rebase { $current } onto { $name }…
repo-core-3416 = upstream gone
repo-core-3417 = checked out in another worktree
repo-core-3418 = has a pull request
repo-core-3419 = hidden from the graph
repo-core-3421 = gone
repo-core-3422 = merged
repo-core-3423 = stale
repo-core-3424 = Filter branches and tags
repo-core-3429 = Select stale or merged branches… once the running operation is done
repo-core-3430 = Recent
repo-core-3431 = Sort the local branches by name, or by their last commit (newest first)
repo-core-3433 = Merge requests
repo-core-3434 = Pull requests
repo-core-3435 = Submodules
repo-core-3436 = Local branches
repo-core-3438 = Remotes
repo-core-3440 = Worktrees
repo-core-3443 = Stashes
repo-core-3444 = Recovery: reflog and lost commits
repo-core-3447 = Start work from an issue assigned to you…
repo-core-3449 = changed since you last looked ·{ " " }
repo-core-3450 = { " " }· draft
repo-core-3451 = { $url }
            Click to show its changes; double-click to open details
repo-core-3452 = { $url }
            Click to show its changes; double-click to open in the browser
repo-core-3453 = Open
repo-core-3454 = Open in the browser
repo-core-3455 = Check out { $branch }
repo-core-3456 = Copy URL
repo-core-3457 = Its branch is checked out here
repo-core-3458 = Opens { $path }
repo-core-3459 = Makes a worktree in { $path } and opens it
repo-core-3460 = Open in worktree
repo-core-3461 = #{ $number }'s branch is checked out here
repo-core-3462 = Open #{ $number } in a worktree
repo-core-3470 = Where HEAD and each branch have been
repo-core-3471 = Commits no branch, tag or stash has any more
repo-core-3472 = RECOVERY
repo-core-3473 = Lost and found by content…
repo-core-3474 = Search remembered text in reflogs, dropped stashes and unreachable blobs
repo-core-3475 = Earlier versions of working changes…
repo-core-3476 = Symbol biography…
repo-core-3477 = Explain line history…
repo-core-3480 = REMOTES
repo-core-3481 = Add a remote…
repo-core-3484 = remote { $name }
repo-core-3485 = { $name }
            Double-click to check out, or drag onto a branch or a commit
repo-core-3486 = Check out as local branch
repo-core-3487 = Fast-forward { $current } to it
repo-core-3488 = Merge into { $current }
repo-core-3489 = Rebase { $current } onto it
repo-core-3490 = Delete from remote…
repo-core-3491 = Delete remote branch
repo-core-3492 = Delete “{ $short }” from { $remote }? This changes the remote for everyone.
repo-core-3495 = Fetch { $name }
repo-core-3497 = Edit…
repo-core-3498 = URLs, refspecs and mirror policy…
repo-core-3499 = Remove…
repo-core-3500 = Remove remote
repo-core-3501 = Remove the remote “{ $name }” ({ $url })? Its branches disappear from this repository; nothing changes on the server.
repo-core-3503 = STASHES
repo-core-3509 = Bring the changes back and keep the stash
repo-core-3510 = Apply { $name }
repo-core-3511 = Bring the changes back and drop the stash
repo-core-3512 = Pop { $name }
repo-core-3513 = Drop…
repo-core-3514 = Drop stash
repo-core-3515 = Delete { $name }? Its changes are gone for good.
repo-core-3516 = SUBMODULES
repo-core-3517 = Update all submodules
repo-core-3518 = Update
repo-core-3519 = Clone it if needed and check out the commit recorded for it
repo-core-3520 = Resolve pointer as a graph…
repo-core-3521 = Open in a new tab
repo-core-3522 = Update it first to clone it
repo-core-3524 = Update it first, or open it on its own machine
repo-core-3525 = Copy path
repo-core-3527 = modified
repo-core-3528 = { "" }
            From { $url }
repo-core-3530 = { "" }
            At { $at }, as recorded
repo-core-3531 = { "" }
            Not cloned yet; Update clones it at { $at }
repo-core-3532 = { "" }
            At { $at }, not the commit recorded here; Update checks that one out
repo-core-3533 = { "" }
            In conflict; at { $at }
repo-core-3534 = { "" }
            Has changes of its own
repo-core-3535 = { "" }
            Double-click to open in a tab
repo-core-3536 = TAGS
repo-core-3538 = Start hotfix from { $name }…
repo-core-3539 = A hotfix branch made here (the next patch version), checked out
repo-core-3540 = Push to { $remote }
repo-core-3541 = Push tag { $name }
repo-core-3542 = Delete from { $remote }…
repo-core-3543 = Delete remote tag
repo-core-3544 = Delete the tag “{ $name }” from { $remote }?
repo-core-3545 = Delete tag…
repo-core-3546 = Delete tag
repo-core-3547 = Delete the local tag “{ $name }”?
repo-core-3549 = Hide in the graph
repo-core-3550 = Show in the graph
repo-core-3552 = The checked-out branch is always shown
repo-core-3553 = Unsolo
repo-core-3554 = Solo
repo-core-3555 = The graph shows only the soloed branches and the checked-out one
repo-core-3556 = Show all branches
repo-core-3557 = soloed
repo-core-3558 = shown
repo-core-3559 = Viewing { $shown } of { $all }
repo-core-3560 = The graph has the commits of the { $what } branches and the checked-out one
repo-core-3561 = Show all
repo-core-3562 = WORKTREES
repo-core-3563 = Add a worktree…
repo-core-3564 = { "" }
            Touched the agent fence; merging its branch asks first:
            { $fenced }
repo-core-3565 = { "" }
            { $e }
            Until it can be, merging its branch asks first.
repo-core-3567 = open here
repo-core-3568 = locked
repo-core-3569 = prunable
repo-core-3571 = missing
repo-core-3572 = bare
repo-core-3573 = detached at { $head }
repo-core-3575 = { "" }
            Locked: { $reason }
repo-core-3576 = { "" }
            Missing: { $reason }
repo-core-3577 = { "" }
            Double-click to open
repo-core-3578 = This worktree is already open here
repo-core-3579 = This worktree is missing
repo-core-3580 = Open in a tab
repo-core-3581 = Open terminal here
repo-core-3583 = This worktree cannot be opened from this machine
repo-core-3584 = Collision matrix…
repo-core-3585 = Which worktree branches would conflict when merged, and the order to land them in
repo-core-3586 = Agent fence…
repo-core-3587 = Paths agents must stay out of, written into their configs and checked
repo-core-3588 = Scope drift…
repo-core-3589 = This worktree has no commit yet
repo-core-3590 = What it changed beyond its task: folders, dependencies, signatures, formatting
repo-core-3591 = Unlock
repo-core-3593 = Lock
repo-core-3595 = { $label } worktree { $name }
repo-core-3596 = Open another worktree to remove this one
repo-core-3597 = Remove worktree
repo-core-3598 = Remove the worktree at { $path }? Its folder is deleted; the branch stays.
repo-core-3599 = Force, even with uncommitted changes or locked
repo-core-3601 = Prune missing worktrees
repo-core-3602 = Prune worktrees
repo-core-3603 = Sparse checkout and downloads
repo-core-3605 = Sparse patterns are limited to a regular UTF-8 file of 1 MiB. Remote metadata reads require python3 with nonblocking/no-follow support.
repo-core-3606 = Cone mode keeps root files and the selected folders. Back up ignored files first; changes are refused while the worktree is dirty.
repo-core-3607 = History reads never download missing objects automatically. A missing-object error means you need to consent here and retry the file/diff. Passive reads require Git with --no-lazy-fetch support (older Git fails closed).
repo-core-3608 = Allow downloads for this operation (may transfer a large history)
repo-core-3609 = Current checkout uses non-cone patterns. Applying will replace them with cone mode.
repo-core-3610 = Check out all folders
repo-core-3611 = Apply checkout selection
repo-core-3612 = Download all reachable history from origin to read old files. This explicit fetch also updates origin's remote-tracking refs; it does not deepen a shallow clone. Other promisor remotes require the terminal.
repo-core-3613 = Download history objects from origin
repo-core-3614 = Working…
repo-core-3615 = Download history objects (consented)
repo-core-3618 = Change sparse checkout
repo-core-3619 = merge base { $rev }
repo-core-3620 = No supported forge remote; local history remains available
repo-core-3621 = Close biography
repo-core-3622 = History depth
repo-core-3623 = Trace function
repo-core-3624 = Cancel trace
repo-core-3625 = Trace cancelled
repo-core-3626 = Rust · Python · JavaScript/JSX · TypeScript/TSX · Go · C/C++ · committed UTF-8 source. Enter a function name, or leave it empty to choose. Rename detection and structural body similarity follow its lifetime.
repo-core-3627 = Choose the function to trace:
repo-core-3629 = Pinned tip { $tip } · { $count } events{ $else }
repo-core-3630 = { " " }· incomplete lineage (see notes)
repo-core-3631 = Scope, uncertainty and limits
repo-core-3633 = Structural body similarity { $confidence }% (heuristic)
repo-core-3634 = Open commit { $commit }
repo-core-3635 = Loading associated pull requests…
repo-core-3636 = No associated pull request found in checked forge evidence.
repo-core-3637 = Follow possible split: { $qualified } · { $path }:{ $first }
repo-core-3638 = Open pinned source
repo-core-3640 = { $String } … [long line clipped; open pinned source]
repo-core-3641 = This function was deleted at this commit.
repo-core-3642 = Previous version
repo-core-3643 = Next version
repo-core-3644 = The shell has exited.
repo-core-3645 = { $what } { $agent }'s { $label } ({ $keys })
repo-core-3646 = { $what } { $label } ({ $keys })
repo-core-3647 = Reload (F5)
repo-core-3648 = File views
repo-core-3651 = Timeline and undo tree ({ $Timeline })
repo-core-3652 = { "" }
            Click now and it starts after { $self }
repo-core-3653 = { $Fetch }
            Last fetched { $elapsed } ago{ $waits }
repo-core-3654 = Fetch all remotes (git fetch --all --prune)
repo-core-3658 = What Pull does, and Fetch all
repo-core-3659 = Submit commits to Gerrit; amend and submit again to update the patch set
repo-core-3661 = Push the current branch (publishes it if it has no upstream yet)
repo-core-3662 = Pull, then push the current branch (git { $mode }, then git push)
            If the pull fails (conflicts, or it can't fast-forward), nothing is pushed
repo-core-3664 = Sync
repo-core-3665 = Branch
repo-core-3666 = Create a branch at HEAD
repo-core-3667 = No commit to branch from yet
repo-core-3668 = Stash everything, staged changes, a snapshot, or picked files
repo-core-3669 = Terminal
repo-core-3670 = Terminal in the repository folder
repo-core-3671 = Activity: each operation's git commands and all they printed
repo-core-3672 = Search commits ({ $Search })
repo-core-3673 = Tour
repo-core-3674 = README, workflow, contributors and setup checklist
repo-core-3675 = Repository doctor: ignore rules, hooks, health, configuration, hidden changes
repo-core-3676 = Open the folder or the repository's page
repo-core-3677 = Open the folder or the repository's page…
repo-core-3678 = Set up SSH…
repo-core-3679 = Review evidence and release coordination…
repo-core-3680 = No remote with a web page
repo-core-3681 = git runs on { $label }, in { $path }
repo-core-3682 = No build status: { $e }
repo-core-3683 = No commits yet
repo-core-3684 = Detached HEAD at { $head }
repo-core-3686 = Abort { $fold }
repo-core-3687 = Click for its git commands and all they printed (Activity)
repo-core-3688 = Stop { $op }, and what it started
repo-core-3689 = Stop { $op }, and what it started; what waits for it won't run
repo-core-3690 = Cancelling { $op }…
repo-core-3693 = then { $names }
repo-core-3694 = Start after { $op }, in this order. Cancel or a failure drops them.
repo-core-3695 = Available once { $op } is done
repo-core-3696 = Show in Explorer
repo-core-3698 = Show in Finder
repo-core-3699 = Open folder
repo-core-3701 = { $s } s
repo-core-3702 = { $conflicts ->
                [one] { $label }: conflicts in { $conflicts } file, resolve it in Changes
               *[other] { $label }: conflicts in { $conflicts } files, resolve them in Changes
            }
repo-core-3703 = { $n ->
                [one] { $label }: { $n } file resolved as before, check it in Changes
               *[other] { $label }: { $n } files resolved as before, check them in Changes
            }
repo-core-3704 = { $conflicts ->
        [one] { $label }: conflicts in { $conflicts } file, { $n } resolved as before: check these, resolve the rest in Changes
       *[other] { $label }: conflicts in { $conflicts } files, { $n } resolved as before: check these, resolve the rest in Changes
    }
repo-core-3747 = Could not start looking for weakened checks: { $e }
repo-core-3749 = Open file
repo-core-3750 = View file
repo-core-3751 = Inspect stored object…
repo-core-3752 = File history
repo-core-3753 = Revert changes to this file
repo-core-3754 = Ignore rules and attributes
repo-core-3755 = Why it is ignored or not, and how git treats it
repo-core-3756 = Ignore
repo-core-3757 = Open in diff tool
repo-core-3758 = Shows this diff in the diff tool of Settings → Git → External tools
repo-core-3759 = Opens the conflict in the merge tool of Settings → Git → External tools; the file is marked resolved when the tool reports success
repo-core-3760 = This file
repo-core-3762 = All .{ $ext } files
repo-core-3764 = Folder { $dir }/
repo-core-3767 = { $p }
            Owners: { $full }
repo-core-3768 = Owners: { $full }
repo-core-3773 = New terminal with…
repo-core-3774 = Close the window (shells keep running)
repo-core-3775 = Hide the panel (shells keep running)
repo-core-3776 = Terminal { $text }
repo-core-3778 = Close (ends the shell)
repo-core-3779 = Close terminal (ends the shell)
repo-core-3780 = Search worktrees
repo-core-3781 = No worktree matches
repo-core-3783 = 1 file with uncommitted changes
repo-core-3784 = { $n ->
            [one] { $n } file with uncommitted changes
           *[other] { $n } files with uncommitted changes
        }
repo-core-3786 = Could not save { $path }: { $e }
repo-core-3787 = { $path } is a symbolic link now: not saved
repo-core-3796 = it is a symbolic link, or not a file, now: not saved
repo-core-3798 = More than 200 conflicts; choose a whole file
repo-core-3799 = { $left ->
                [one] { $left } conflict still unresolved in { $path }
               *[other] { $left } conflicts still unresolved in { $path }
            }
repo-core-3800 = Upstream ({ $ours-label })
repo-core-3801 = Your commit ({ $theirs-label })
repo-core-3802 = Ours ({ $ours-label })
repo-core-3803 = Theirs ({ $theirs-label })
repo-core-3805 = The message changed while prepare-commit-msg ran: run it again
repo-core-3806 = This repository has no prepare-commit-msg hook
repo-core-3807 = The message was replaced.
repo-core-3808 = the archive's directory is damaged, so some parts may be missing
repo-core-3814 = only its first { $MAX-TEXT } MB of text is compared
repo-core-3833 = slide { $n } couldn't be read
repo-core-3834 = — Slide { $n } —
            { $text }
            { "" }
repo-core-3847 = its shared strings couldn't be read
repo-core-3849 = sheet { $n } couldn't be read
repo-core-3850 = — Sheet { $n } —
            { "" }
repo-core-3873 = streams past its first { $PDF-BUDGET } MB inflated weren't read
repo-core-3899 = not a file
repo-core-3900 = the old version
repo-core-3901 = the new version
repo-core-3902 = The text of { $which } can't be read, so it isn't compared.
repo-core-3903 = The text of { $which } is incomplete ({ $gaps }), so this comparison may show changes that aren't there.
repo-core-3904 = The texts are too unlike to match line by line: everything between the first and the last change is shown removed and added.
repo-core-3905 = is too big to read
repo-core-3906 = has a member list that can't be read
repo-core-3907 = has a damaged member list
repo-core-3908 = The { $which } version { $why }, so its members aren't compared.
repo-core-3909 = The { $which } version has more than one member named { $d }: the first of each is compared.
repo-core-3913 = Decryption stopped. Return to the encrypted patch and try again.
repo-core-3914 = Decryption cancelled.
repo-core-3915 = Could not read both encrypted sides within 2 MiB. Refresh the encrypted patch.
repo-core-3916 = The comparison changed while decrypting. Refresh the encrypted patch.
repo-core-3917 = The comparison changed while decrypting. Refresh the encrypted patch and choose Show decrypted again.
repo-core-3918 = Encrypted source reading exceeded 15 seconds. Retry when the connection and background workers are available.
repo-core-3919 = Encrypted source reader stopped.
repo-core-3920 = No supported SOPS metadata was found. Use the encrypted patch.
repo-core-3921 = This data + sops envelope could be a JSON document or binary content. Choose JSON document or Binary envelope above, then retry. The filename cannot identify the format.
repo-core-3922 = SOPS did not return the chosen JSON document format.
repo-core-3923 = SOPS could not decrypt this side. Check the installed sops version and this computer's local keys/permissions; the encrypted patch is still available. Tool diagnostics are hidden because they can contain secrets.
repo-core-3929 = Decryption exceeded 20 seconds and was stopped. The encrypted patch is still available.
repo-core-3931 = SOPS output exceeded its size limit (2 MiB stdout, 32 KiB diagnostics). The encrypted patch is still available.
repo-core-3932 = Could not start local SOPS. Install SOPS 3.10 or later on this computer and check its local keys/permissions. The encrypted patch is still available.
repo-core-3933 = Decrypted comparison supports UTF-8 text within 2 MiB per side; binary output is not displayed.
repo-core-3934 = Decrypted comparison supports UTF-8 text only.
repo-core-3935 = Decrypted text exceeded 20,000 lines or 8,192 characters per line. Use the encrypted patch.
repo-core-3949 = line { $a }
repo-core-3950 = lines { $a }–{ $z }
repo-core-3951 = Moved from { $true } to { $false }
repo-core-3952 = only re-indented on the way
repo-core-3953 = unchanged on the way
repo-core-3954 = 1 line edited on the way
repo-core-3955 = { $n ->
            [one] { $n } line edited on the way
           *[other] { $n } lines edited on the way
        }
repo-core-3956 = Go to where it is now
repo-core-3957 = Scroll the diff to the added block
repo-core-3958 = Go to where it was
repo-core-3959 = Scroll the diff to the removed block
repo-core-3960 = Invalid registry package identity
repo-core-3964 = Published licence/changelog lookup is available for crates.io and npm packages; this source is unavailable
repo-core-3965 = Offline mode blocks package metadata lookup
repo-core-3969 = Package registry returned HTTP { $n }
repo-core-3970 = Package registry request failed
repo-core-3971 = Package registry redirected the request; refused
repo-core-3972 = Could not read package metadata
repo-core-3973 = Package download exceeds 8 MiB
repo-core-3974 = Package omits a regular Cargo.toml
repo-core-3975 = Invalid published Cargo.toml
repo-core-3976 = Published manifest omits package
repo-core-3977 = Published package identity mismatch
repo-core-3979 = declared licence file is missing, not a regular file, or unavailable in the archive
repo-core-3980 = declared licence file path is unsafe or outside the package
repo-core-3981 = Package omits a regular package.json
repo-core-3982 = Invalid published package.json
repo-core-3984 = licence declaration uses an unsupported representation; its meaning is unknown
repo-core-3985 = Unsupported package registry
repo-core-3986 = Invalid registry metadata
repo-core-3987 = Registry package identity mismatch
repo-core-3989 = Registry omits published archive
repo-core-3990 = Registry archive points outside the public npm registry
repo-core-3991 = declared file { $path }{ $else }
repo-core-3992 = { " " }(content unavailable)
repo-core-3993 = not declared in published manifest
repo-core-3994 = { $side }: published archive · { $archive-url }
repo-core-3995 = { $side } licence: { $license-summary }
repo-core-3996 = { $side } declared licence file: { $path }
repo-core-3997 = { $side }: no readable recognised changelog in retained metadata; other filenames may exist.
repo-core-3998 = { $side } changelog: { $path }
repo-core-3999 = { $side }: { $path } · { $reason }
repo-core-4000 = not recorded as a supported public-registry package
repo-core-4001 = unknown ({ $error })
repo-core-4002 = Metadata unavailable: the lockfile does not identify a supported public registry; no package name was sent
repo-core-4003 = Profile changed; select the package again
repo-core-4004 = Package metadata task cancelled or stopped
repo-core-4005 = { $name }: published licences and changelogs
repo-core-4006 = Reading the exact published versions…
repo-core-4007 = Licence{ $else }: { $answer } → { $answer2 }
repo-core-4008 = { " " }changed
repo-core-4009 = { $side }: metadata unavailable for this source
repo-core-4010 = { $side }: { $error }
repo-core-4011 = Close / cancel
repo-core-4012 = Retry metadata
repo-core-4013 = Reading the exact published versions (up to 12 seconds per request)…
repo-core-4015 = { $side }: published archive
repo-core-4017 = { $side }: changelog unavailable — no readable recognised filename in retained metadata; other filenames may exist.
repo-core-4021 = Show the changes as lines
repo-core-4022 = Show the changes as prose: wrapped paragraphs, the words that changed marked
repo-core-4023 = Show the changes by key: the keys added, removed, changed and moved
repo-core-4024 = Show the changes as a table: rows and cells added, removed and changed
repo-core-4025 = Show the data this file tracks, before and after
repo-core-4026 = Show the dependency changes: packages upgraded, downgraded, added, removed and relicensed
repo-core-4027 = Show the API contract changes: what breaks the API's clients, and what was added
repo-core-4028 = as prose
repo-core-4029 = by key
repo-core-4030 = as API contract changes
repo-core-4031 = as dependency changes
repo-core-4032 = as a table
repo-core-4033 = as the data it tracks
repo-core-4034 = the file is over { $MAX-SIDE-BYTES } MB
repo-core-4035 = the file isn't UTF-8 text
repo-core-4045 = Can't show this { $how }: { $note }. Showing the plain diff.
repo-core-4046 = Syntax
repo-core-4047 = Compare syntax with Difftastic (difft), installed locally. SSH/WSL use the patch. Use the patch to stage changes.
repo-core-4048 = Before { $number } · { $text }
repo-core-4049 = After { $number } · { $text }
repo-core-4069 = Couldn't prepare syntax comparison: { $e }
repo-core-4070 = Install Difftastic (difft) on the repository's machine to compare syntax.
repo-core-4071 = Difftastic's output was too large to show.
repo-core-4072 = Difftastic couldn't compare these files: { $trim }
repo-core-4074 = Couldn't create a private folder for syntax comparison.
repo-core-4075 = Couldn't start Difftastic. Install Difftastic (difft) to compare syntax: { $e }
repo-core-4076 = What you typed here last time couldn't be restored: { $e }
repo-core-4077 = The interactive rebase you were setting up is back: start it, or cancel it
repo-core-4078 = The interactive rebase you were setting up was saved by another version of gitcraft and couldn't be restored
repo-core-4081 = Merge commits can't be cherry-picked here
repo-core-4082 = Drop it on the checked-out branch to cherry-pick it, or on a commit of that branch to move it
repo-core-4083 = Only the checked-out branch is merged into or rebased here
repo-core-4084 = { $op } is still running, drop when it's done
repo-core-4085 = Finish or abort the { $to-lowercase } first
repo-core-4102 = The regular expression is too big; try a simpler one
repo-core-4103 = it can't be read
repo-core-4104 = Not a valid regular expression: { $why }
repo-core-4105 = searching… { $max }%
repo-core-4106 = { $i } of { $n }{ $more }
repo-core-4107 = { $i } of { $n }…
repo-core-4108 = { $n ->
            [one] { $n }{ $more } result
           *[other] { $n }{ $more } results
        }
repo-core-4109 = { $n ->
            [one] { $n } result…
           *[other] { $n } results…
        }
repo-core-4110 = result
repo-core-4111 = results
repo-core-4112 = There are no commits yet
repo-core-4113 = Fold staged changes into this commit
repo-core-4114 = Put the staged changes into this commit, rewriting the commits after it
repo-core-4115 = Untick Amend last commit to fold into an older commit
repo-core-4116 = Fold…
repo-core-4117 = Fold the staged changes into an older commit of this branch
repo-core-4118 = Fold the staged changes into:
repo-core-4119 = Older commits: right-click them in the graph
repo-core-4120 = Fold staged changes: { $e }
repo-core-4121 = { $label }: { $busy } is still running, try again when it's done
repo-core-4122 = { $label }: a { $to-lowercase } is in progress now: finish or abort it first
repo-core-4124 = { $label }: something else started, try again
repo-core-4125 = { $label }: nothing was changed. The staged changes undo what a later commit does, so they would have been lost in the commit; they are staged as before.
repo-core-4126 = { $label }: the result lost some of the staged changes, and going back failed ({ $e }). They are all in ORIG_HEAD, the fixup! commit.
repo-core-4127 = { $label } stopped on a conflict: resolve it and Continue rebase, or Abort to go back with the changes staged
repo-core-4128 = { $label } (stopped partway)
repo-core-4129 = { $label }: the rebase didn't run, so the changes are in a commit on top. Ctrl+Z stages them again.
repo-core-4130 = Abort rebase: { $e }
repo-core-4131 = Stop the fold and go back to where it started? The branch is put back as it was, with the changes staged again. Conflicts resolved so far are lost, and can't be brought back.
repo-core-4132 = Stage the changes of { $fold } again
repo-core-4133 = Take back { $fold }
repo-core-4137 = This repository has no commits yet.
repo-core-4142 = Loading older commits…
repo-core-4143 = A patch from git format-patch becomes commits (git am), any other diff goes on the working tree (git apply)
repo-core-4144 = Changes { $name } adds to { $current }
repo-core-4145 = Compare { $current } with { $name }
repo-core-4146 = Every difference between the two
repo-core-4147 = { $count ->
            [one] ⋯ { $count } intervening commit · expand
           *[other] ⋯ { $count } intervening commits · expand
        }
repo-core-4148 = Expand this omitted history. Dashed lanes show continuous boundary runs; internal merges remain hidden until expanded.
repo-core-4149 = Find the interaction…
repo-core-4150 = When this branch and the checked-out one pass alone but fail together: tests subsets of their commits until it finds those that fail together
repo-core-4151 = Find the interaction of { $current } and { $name }…
repo-core-4153 = A search is going on already
repo-core-4154 = Internal error: the search stopped
repo-core-4155 = { $name } · the search of { $a } and { $b } is done
repo-core-4157 = Could not verify the reviewed push's destinations; command not started: { $error }
repo-core-4158 = Invalid sign-in route proof
repo-core-4160 = { "" }
            If the pull had started merging or rebasing, that is left in progress: Abort in the toolbar goes back to where the branch was.
repo-core-4161 = { $why }: turn it off in the toolbar or Settings → Git.
repo-core-4164 = HEAD has moved since (it is at { $now }, not { $expected }): resetting it would lose what was done after
repo-core-4166 = HEAD moved while preparing this operation; reopen its preview. No operation was run.
repo-core-4167 = Before checkout / rebase
repo-core-4168 = Working draft safety check: { $error }
repo-core-4178 = { $rev } isn't what this made any more (it was made again since, with other changes): going ahead would lose those
repo-core-4179 = a detached HEAD
repo-core-4180 = something else
repo-core-4181 = { " " }in the worktree at { $d }
repo-core-4182 = { $now } is checked out{ $place } now, not { $target }: going ahead would change it instead
repo-core-4183 = The newest stash is not the one this made any more
repo-core-4184 = { $what } has commits now: going ahead would go on top of them
repo-core-4185 = { $what } has moved since (it is at { $now }, not { $value }): going ahead would lose what was done after
repo-core-4187 = How to reset…
repo-core-4188 = Branch to push…
repo-core-4189 = Branch…
repo-core-4193 = Search
repo-core-4194 = graph or visible lines
repo-core-4197 = Run Actions locally…
repo-core-4199 = Email patches…
repo-core-4201 = Worktree dev container…
repo-core-4203 = Create branch…
repo-core-4204 = Create tag…
repo-core-4206 = CI view
repo-core-4208 = { $MAX-WHAT } — { $MAX-REPO } — GitCraft
repo-core-4210 = Pop out the view shown
repo-core-4212 = Pop out the terminal
repo-core-4213 = Bring back the popped-out views
repo-core-4215 = Diff of { $path }
repo-core-4216 = Terminal ({ $shell })
repo-core-4217 = Pull request #{ $number }: { $title }
repo-core-4218 = Pull request #{ $number }
repo-core-4219 = Pull request
repo-core-4220 = Pipelines
repo-core-4221 = Workflow runs
repo-core-4222 = { $noun } of { $sha }
repo-core-4223 = The conflict editor is open in the main window.
repo-core-4225 = There's no { $noun } open
repo-core-4226 = No view is in a window of its own
repo-core-4227 = Nothing to pop out: finish or close the conflict editor first
repo-core-4228 = Nothing to pop out: finish or close the interactive rebase first
repo-core-4229 = Nothing else to pop out: open a diff, a pull request or the CI view first
repo-core-4230 = Nothing to pop out: open a diff, a pull request or the CI view first
repo-core-4231 = No diff open. Click a file in the main window to show its diff here.
repo-core-4232 = The terminal is hidden; its shells keep running.
repo-core-4233 = No pull request open. Open one in the main window to show it here.
repo-core-4234 = No CI view open. Open the builds of a commit to show them here.
repo-core-4235 = Show the terminal
repo-core-4236 = Close this window
repo-core-4238 = Bring the { $noun } back into the main window
repo-core-4239 = Pop the { $noun } out into its own window
repo-core-4240 = Drag to move the commit
repo-core-4241 = commit message
repo-core-4242 = editor
repo-core-4243 = review draft
repo-core-contained-branches = { $count ->
                [0] no branches
                [one] { $count } branch
               *[other] { $count } branches
            }
repo-core-contained-tags = { $count ->
                [0] no tags
                [one] { $count } tag
               *[other] { $count } tags
            }
repo-core-table-rows-changed = { $count ->
                [one] { $count } row changed
               *[other] { $count } rows changed
            }
repo-core-table-rows-added = { $count ->
                [one] { $count } row added
               *[other] { $count } rows added
            }
repo-core-table-rows-removed = { $count ->
                [one] { $count } row removed
               *[other] { $count } rows removed
            }
repo-core-table-rows-renamed = { $count ->
                [one] { $count } row renamed
               *[other] { $count } rows renamed
            }
repo-core-table-columns-changed = { $count ->
                [one] { $count } column changed
               *[other] { $count } columns changed
            }
repo-core-table-columns-added = { $count ->
                [one] { $count } column added
               *[other] { $count } columns added
            }
repo-core-table-columns-removed = { $count ->
                [one] { $count } column removed
               *[other] { $count } columns removed
            }
repo-core-table-columns-renamed = { $count ->
                [one] { $count } column renamed
               *[other] { $count } columns renamed
            }
repo-core-toolbar-behind = { $name }, { $count } behind
repo-core-toolbar-ahead = { $name }, { $count } ahead
repo-core-line-range-one = Line { $range }
repo-core-line-range-many = Lines { $range }
repo-core-fold-staged = { $fixup ->
                [yes] { $count ->
                    [one] The staged file will fix up this commit:
                   *[other] The { $count } staged files will fix up this commit:
                }
               *[no] { $count ->
                    [one] The staged file will be folded into this commit:
                   *[other] The { $count } staged files will be folded into this commit:
                }
            }
repo-core-operation-dco = Sign off the commits of { $branch }
repo-core-operation-dco-here = Sign off the commits of { $branch } (here only)
repo-core-change-unchanged = Unchanged
repo-core-change-added = Added
repo-core-change-removed = Removed
repo-core-change-modified = Modified
repo-core-change-changed = Changed
repo-core-change-reordered = Reordered
repo-core-change-breaking = Breaking
repo-core-change-upgraded = Upgraded
repo-core-change-downgraded = Downgraded
repo-core-change-relicensed = Relicensed
repo-core-not-executed = not run
repo-core-count-changed = { $count } changed
repo-core-cells-changed = { $count ->
                [one] { $count } cell changed
               *[other] { $count } cells changed
            }
repo-core-count-added = { $count } added
repo-core-cells-added = { $count ->
                [one] { $count } cell added
               *[other] { $count } cells added
            }
repo-core-count-removed = { $count } removed
repo-core-cells-removed = { $count ->
                [one] { $count } cell removed
               *[other] { $count } cells removed
            }
repo-core-count-moved = { $count } moved
repo-core-cells-moved = { $count ->
                [one] { $count } cell moved
               *[other] { $count } cells moved
            }
repo-core-count-upgraded = { $count } upgraded
repo-core-cells-upgraded = { $count ->
                [one] { $count } cell upgraded
               *[other] { $count } cells upgraded
            }
repo-core-count-downgraded = { $count } downgraded
repo-core-cells-downgraded = { $count ->
                [one] { $count } cell downgraded
               *[other] { $count } cells downgraded
            }
repo-core-count-licenses = { $count } with licences
repo-core-cells-licenses = { $count ->
                [one] { $count } cell with licences
               *[other] { $count } cells with licences
            }
repo-core-table-old-line = Go to line { $line } of the file before the change
repo-core-cancelled = cancelled
repo-core-cancelled-locks = cancelled, but git left { $locks } behind
            { $amount ->
                [one] Until it is deleted, git refuses to work in this repository. Delete it if no other git program is running here.
               *[many] Until them are deleted, git refuses to work in this repository. Delete them if no other git program is running here.
            }
repo-core-forge-fallback = The forge
repo-core-rebase-commit-accessible = Rebase commit { $hash }, { $summary }, { $author }, { $step }
repo-core-cell-kind-code = code
repo-core-cell-kind-markdown = markdown
repo-core-cell-kind-raw = raw
repo-core-panel-diff = diff
repo-core-panel-terminal = terminal
repo-core-panel-pr = pull request
