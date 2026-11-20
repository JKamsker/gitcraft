# File metadata edits without a terminal: the executable bit, case-only renames, and the
# mode and name changes a diff shows apart from its lines (#173, item 13).

file-metadata-executable-menu = Executable bit in the index…
file-metadata-executable-menu-tip = Set or clear the executable bit of the file's index entry. Only that mode changes: staged and unstaged contents stay as they are. Works on Windows too, where files on disk have no executable bit.
file-metadata-case-menu = Change letter case of the name…
file-metadata-case-menu-tip = Rename the file to the same name in other letter case, safely on case-insensitive filesystems (Windows, macOS). Edits move with it.
file-metadata-executable-title = Executable bit in the index
file-metadata-case-title = Change letter case of the name
file-metadata-apply = Apply
file-metadata-rename = Rename
file-metadata-executable-change = { $executable ->
    [true] Index mode { $from } → { $to }: executable
   *[other] Index mode { $from } → { $to }: not executable
}
file-metadata-executable-kept = Only the index entry's mode changes. The file's contents, staged or not, and the working-tree file stay as they are; commit to record the new mode.
file-metadata-case-kept = The file moves through a temporary name, so this works on case-insensitive filesystems. Staged and unstaged edits move with it unchanged.
file-metadata-refused-conflicted = The file has a conflict. Resolve it first.
file-metadata-refused-untracked = The file isn't in the index.
file-metadata-refused-not-regular = Only regular files have an executable bit (this one's mode is { $mode }).
file-metadata-refused-bad-name = Type a file name, without a folder.
file-metadata-refused-same-name = Type the name in other letter case.
file-metadata-refused-not-case-only = Only the letter case can change here. Use a terminal or your editor for other renames.
file-metadata-refused-missing = The file is deleted in the working tree.
file-metadata-refused-collision = { $name } is already there in other letter case.
file-metadata-refused-no-temp = No free temporary name was found in the folder.
file-metadata-mode-regular = file
file-metadata-mode-executable = executable
file-metadata-mode-link = symbolic link
file-metadata-mode-submodule = submodule
file-metadata-diff-mode = Mode { $old } ({ $old-kind }) → { $new } ({ $new-kind })
file-metadata-diff-renamed = { $similarity ->
    [none] Renamed from { $from }
   *[other] Renamed from { $from } · { $similarity }% similar
}
file-metadata-diff-copied = { $similarity ->
    [none] Copied from { $from }
   *[other] Copied from { $from } · { $similarity }% similar
}
file-metadata-changed = The file changed since the dialog opened. Check what is shown now and confirm again.
file-metadata-file-mode-warning = Git tracks file modes here (core.fileMode), and the file on disk keeps its old permission: Git shows the opposite change as unstaged, and staging all files or commit -a would undo this. Change the file's permission too (chmod).
file-metadata-read-failed = The file couldn't be read. Close the dialog and open it again.
file-metadata-patch-placeholder = <mode patch, written when confirmed>
file-metadata-rename-rolled-back = Couldn't rename { $via } to { $to }; moved it back to { $from }
