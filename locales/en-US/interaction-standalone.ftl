# Standalone file-tool presentation. Preserve literal paths/argv/markers and opaque diagnostics.

interaction-standalone-base = Base

interaction-standalone-binary-preview = { $count ->
        [one] Binary/non-UTF8 · one byte
       *[other] Binary/non-UTF8 · { $count } bytes
    }
    { $preview }

interaction-standalone-cancel-tool = Cancel file tool

interaction-standalone-choose-whole-file = Choose a whole file to produce an exact byte result.

interaction-standalone-close-error = Close with error

interaction-standalone-compare-title = Compare files

interaction-standalone-computed-explanation = Replaces unsaved edits in this window. MERGED on disk stays unchanged until Save and finish.

interaction-standalone-computed-heading = Replace working result with computed merge

interaction-standalone-done = Done

interaction-standalone-error-access-limit = Output access metadata exceeds 1 MiB

interaction-standalone-error-access-limit-unchanged = Output access metadata exceeds 1 MiB; the original was not changed

interaction-standalone-error-access-read-changed = Output access metadata changed while reading; reopen before saving

interaction-standalone-error-access-snapshot = Cannot snapshot output access metadata: { $detail }

interaction-standalone-error-acl-identity-changed = Output identity changed during native ACL read

interaction-standalone-error-acl-limit = Native ACL exceeds bounded snapshot limits

interaction-standalone-error-atomic-install = This filesystem cannot install a result atomically without overwrite: { $detail }

interaction-standalone-error-before-install = The output, inputs or access metadata changed before installation

interaction-standalone-error-binary-inputs = Binary or non-UTF8 inputs: choose a whole side to preserve exact bytes

interaction-standalone-error-choose-input = Choose a whole input or prepare a text result

interaction-standalone-error-clone-metadata = This filesystem cannot safely clone output ACL/metadata: { $detail }

interaction-standalone-error-close-writers = Close other writers before saving: { $detail }

interaction-standalone-error-comparison-cancelled = File comparison cancelled

interaction-standalone-error-comparison-limit = File comparison is limited to 2 MiB per file

interaction-standalone-error-computed-encoding = The computed merge is not UTF-8

interaction-standalone-error-computed-limit = Computed result exceeds 2 MiB; choose a whole input

interaction-standalone-error-create-reservation = Could not create save reservation: { $detail }

interaction-standalone-error-descriptor-response = Invalid access descriptor response

interaction-standalone-error-during-save = Files changed during the save. The current result and displaced bytes were preserved; inspect them before continuing

interaction-standalone-error-executable = Could not find this executable: { $detail }

interaction-standalone-error-expected-invocation = Expected a standalone file invocation

interaction-standalone-error-expected-tool = Expected a standalone file tool

interaction-standalone-error-file-arity = { $count ->
        [one] Expected one file path
       *[other] Expected { $count } file paths
    }
    { $usage }

interaction-standalone-error-file-read-changed = { $path } changed while reading; reopen the file tool

interaction-standalone-error-filesystem-metadata = This filesystem cannot expose the output's access metadata safely; the original was not changed

interaction-standalone-error-foreign-lock = Foreign save lock captured during release; retained in recovery and restored only into absence

interaction-standalone-error-index-metadata-preserve = Index access metadata could not be preserved

interaction-standalone-error-index-policy-preserve = Full index access policy could not be preserved

interaction-standalone-error-index-policy-unsupported = Index access policy is unsupported on this platform

interaction-standalone-error-input-changed = Input { $path } changed; reopen before saving

interaction-standalone-error-line-limit = More than 20,000 lines in an input; choose a whole side

interaction-standalone-error-lock-changed = Save lock changed or disappeared; foreign entries preserved

interaction-standalone-error-lock-parent = Save lock parent changed during release; captured entry retained

interaction-standalone-error-lock-release = Save lock release uncertain: { $detail }

interaction-standalone-error-merge-cancelled = File merge cancelled

interaction-standalone-error-merged-before-metadata = MERGED changed before metadata preparation

interaction-standalone-error-merged-changed = MERGED changed since opening; reopen before saving. The current file was preserved.

interaction-standalone-error-metadata-list-changed = Output metadata changed while listing

interaction-standalone-error-metadata-overflow = Output metadata overflow

interaction-standalone-error-metadata-read-changed = Output metadata changed while reading

interaction-standalone-error-native-attributes = Unsupported native output attributes; the original was not changed

interaction-standalone-error-native-basename = Expected one native file basename

interaction-standalone-error-no-computed-merge = No computed text merge is available

interaction-standalone-error-no-output-identity = No output identity

interaction-standalone-error-no-output-parent = No output parent directory

interaction-standalone-error-no-overwrite = Could not install without overwrite; a concurrent destination was preserved: { $detail }

interaction-standalone-error-no-pinned-source = No pinned source

interaction-standalone-error-no-result-parent = No parent for merged result

interaction-standalone-error-no-source-identity = No source identity

interaction-standalone-error-no-source-policy = No captured source access policy

interaction-standalone-error-open-file = Could not open { $path }: { $detail }

interaction-standalone-error-open-parent = Could not open output parent: { $detail }

interaction-standalone-error-output-acl = Could not preserve output ACL: { $detail }

interaction-standalone-error-output-alias = MERGED aliases an input file; choose a separate output so inputs remain read-only

interaction-standalone-error-output-empty-device = An empty-input device cannot be the merged output

interaction-standalone-error-output-filename = The merged result needs a file name

interaction-standalone-error-output-limit = Merged output exceeds 2 MiB

interaction-standalone-error-output-link = The merge output is a link/reparse point

interaction-standalone-error-output-ownership = Could not preserve output ownership: { $detail }

interaction-standalone-error-output-plain-file = MERGED must be a plain file, not a link/device: { $detail }

interaction-standalone-error-output-readonly = The merged output is read-only

interaction-standalone-error-output-type = The output changed type

interaction-standalone-error-parent-identity = The output parent changed identity; reopen the file tool

interaction-standalone-error-parent-type = The output parent contains a link or is not a directory

interaction-standalone-error-platform-metadata = Safe preservation of this platform's access metadata is unavailable; the original output was not changed

interaction-standalone-error-prepare-merge = Git could not prepare a three-way merge; choose a whole input or edit existing MERGED

interaction-standalone-error-prepared-policy = Prepared output did not preserve the captured access policy; original unchanged

interaction-standalone-error-preserve-metadata = Could not preserve output access metadata: { $detail }

interaction-standalone-error-previous-result = Could not preserve the previous result: { $detail }

interaction-standalone-error-recovery-closed = The recovery directory is closed

interaction-standalone-error-recovery-location = { $detail }. Recovery directory: { $path }

interaction-standalone-error-recovery-reserve = Could not reserve a recovery directory

interaction-standalone-error-registration-partial = Installed { $installed } of { $total } settings; could not write { $key }: { $detail }. Retry or apply the previewed commands manually.

interaction-standalone-error-regular-file = { $path } must be a regular file within 2 MiB

interaction-standalone-error-replacement-acl = Replacement ACL differs from the captured output policy

interaction-standalone-error-replacement-identity = Unsafe replacement file identity

interaction-standalone-error-replacement-reparse = Replacement is a reparse point

interaction-standalone-error-repository-arity = Expected one repository path
    { $usage }

interaction-standalone-error-reserve-result = Could not reserve the result (another save may own its lock): { $detail }

interaction-standalone-error-save-and-release = { $error }. { $release }

interaction-standalone-error-save-identity = MERGED or its parent changed during saving; no successful save was reported

interaction-standalone-error-save-maybe-installed = Save may have installed the result; { $detail }

interaction-standalone-error-system-root-absolute = SystemRoot must be an absolute native path

interaction-standalone-error-system-root-unavailable = SystemRoot is unavailable; cannot safely preserve output access rules

interaction-standalone-error-unknown-option = Unknown option: { $option }
    { $usage }

interaction-standalone-error-windows-copy-response = Unexpected Windows metadata-copy response

interaction-standalone-error-windows-metadata = Windows access metadata could not be preserved; original unchanged: { $detail }

interaction-standalone-error-worker-stopped = The file-tool worker stopped

interaction-standalone-inputs-heading = Show Base / Local / Remote inputs

interaction-standalone-keep-merged = Keep current MERGED

interaction-standalone-local = Local

interaction-standalone-merge-title = Merge files

interaction-standalone-merged-explanation = Existing MERGED starts unchanged. Saving retains the previous file in a sibling recovery folder.

interaction-standalone-merged-path = MERGED: { $path }

interaction-standalone-previous-location = Previous MERGED preserved at { $path }

interaction-standalone-reading = Reading file snapshots…

interaction-standalone-registration-complete = { $count ->
        [one] Registered one named-tool setting. Use git difftool --tool=gitcraft or git mergetool --tool=gitcraft.
       *[other] Registered { $count } named-tool settings. Use git difftool --tool=gitcraft or git mergetool --tool=gitcraft.
    }

interaction-standalone-registration-copy = Copy Git tool commands

interaction-standalone-registration-defaults = Also make gitcraft the default diff and merge tool

interaction-standalone-registration-explanation = Writes only the settings below to this machine's global Git config. No repository, profile, merge or file comparison runs.

interaction-standalone-registration-pending = Writing the listed Git settings…

interaction-standalone-registration-register = Register named Git tools

interaction-standalone-remote = Remote

interaction-standalone-save-finish = Save and finish

interaction-standalone-saving = Finishing the guarded save. Closing waits for its result.

interaction-standalone-snapshot-summary = Read-only input snapshots · 2 MiB per file · no repository actions

interaction-standalone-truncated-preview = { $preview }
    … input preview truncated

interaction-standalone-usage = Usage:
      gitcraft [REPOSITORY]
      gitcraft --diff [--] BEFORE AFTER
      gitcraft --merge [--] BASE LOCAL REMOTE MERGED
      gitcraft --mergetool [--] BASE LOCAL REMOTE MERGED

    File tools open their own window and never discover a repository.
    Diff is read-only. Merge writes MERGED only after Save and finish.
    Exit: 0 completed, 1 cancelled, 2 usage/read/save failure.
    Use -- before a repository name beginning with -. { "\u000A" }

interaction-standalone-use-base = Use Base file

interaction-standalone-use-computed = Use computed merge

interaction-standalone-use-local = Use Local file

interaction-standalone-use-remote = Use Remote file

interaction-standalone-window-title = GitCraft file tools
