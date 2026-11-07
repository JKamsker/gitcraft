# Compare and merge files without a repository

Run these commands in a terminal, or register the tools in **Settings → Git → Git file tools**:

```sh
gitcraft --diff BEFORE AFTER
gitcraft --merge BASE LOCAL REMOTE MERGED
# --mergetool is an alias for --merge.
```

Paths are native command-line arguments. Quote spaces and shell punctuation as your shell
requires. An optional `--` before the file arguments separates option-looking names.
No repository, profile, forge, filter or decryption mode is used. Diff is read-only;
`/dev/null` (or bare `NUL` on Windows) represents an empty input. Other missing inputs
produce an error rather than opening a repository. MERGED may be a new file.

Text alignment and computed merges use the installed Git with `--no-lazy-fetch` support,
with private temporary files and isolated configuration. Missing/incompatible Git is
reported explicitly; binary whole-file choices and existing-result editing remain available.

The merge window starts from existing MERGED, preserving earlier manual work. If it does
not exist, the window prepares a three-way merge from BASE, LOCAL and REMOTE. **Use computed
merge** deliberately replaces only the unsaved result in the window. Resolve conflict
cards or edit the result, then choose **Save and finish**. Whole-file choices preserve
binary/non-UTF8 bytes. Input files are never written; an output that aliases an input is
refused. Limits are 2 MiB per compared/default file stream, 20,000 text input lines and
200 editable conflict blocks; larger conflict sets offer whole-file choices.

Closing a successfully loaded diff returns 0, so Git can open the next comparison.
Explicit Cancel/Escape returns 1. Closing an unsaved merge returns 1. A merge returns 0
only after an explicit successful save. Usage and read/save errors return 2; the window
shows errors and offers **Close with error**. Closing while saving waits for the owned
transaction's result. The tool never stages a file or resolves a Git operation itself.

## Guarded saves and recovery

The tool checks output/input snapshots and parent identity before saving, reserves its
own lock, and prepares the result in an exclusively created sibling directory. Existing
MERGED is moved into that recovery directory, then checked again. Installation uses an
atomic create-only hard link: a concurrently recreated destination is never overwritten.
If anything changed, success is refused; restoration is also create-only. This protocol
can briefly leave the destination absent while its original is safely retained.

The previous file remains at `.gitcraft-merge-<id>/previous` beside MERGED, and its location
is reported on success or failure. Late writes through old handles go to that retained
inode. Inspect recovery files before deleting them yourself; they are deliberately not
automatically removed. Failed saves can leave a recovery directory, including when a new
concurrent destination prevents restoration. No fallback silently truncates the current
file. Filesystems without the required hard-link operation refuse saving before moving
the original. Links/devices are refused as merge outputs; input links are read-only.

Permission handling uses safe platform APIs. Linux preserves bounded user/access-control
extended attributes, with a 1 MiB metadata limit; edited content does not regain privileged
mode bits, capabilities or stale integrity signatures. Windows copies the stable source's
native security metadata/streams before replacing the default stream, and requires other
writers to release it during that preparation. macOS uses native file cloning to preserve
ACLs and refuses unsupported filesystems. The retained original keeps its original
metadata. Permission/metadata failures leave it preserved and do not report success.

## Git integration

Settings show the exact commands before **Register named Git tools** writes them. The
named configuration uses the current executable, quoted for Git's shell, and sets:

```gitconfig
[difftool "gitcraft"]
    cmd = 'gitcraft' --diff -- "$LOCAL" "$REMOTE"
[mergetool "gitcraft"]
    cmd = 'gitcraft' --mergetool -- "$BASE" "$LOCAL" "$REMOTE" "$MERGED"
    trustExitCode = true
```

The displayed executable path replaces `gitcraft` above. Registration changes only these
named settings unless **Also make gitcraft the default diff and merge tool** is checked.
The default checkbox additionally writes `diff.tool=gitcraft` and `merge.tool=gitcraft`.
Existing unrelated Git settings stay in place. A failed registration identifies how many
settings were written and can be retried. No configuration is installed on CLI startup.

Use the named tools without changing defaults:

```sh
git difftool --tool=gitcraft --trust-exit-code
git mergetool --tool=gitcraft
```

To remove the named registration:

```sh
git config --global --unset-all difftool.gitcraft.cmd
git config --global --unset-all mergetool.gitcraft.cmd
git config --global --unset-all mergetool.gitcraft.trustExitCode
```

If you selected defaults, deliberately restore your preferred `diff.tool` and `merge.tool`
as well. Development checks use owned temporary configuration files and file fixtures;
they do not install settings in the developer's real Git config or run real user merges.
