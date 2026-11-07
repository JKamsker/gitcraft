# Standalone file tools implementation plan

Status: implementation plan, unvalidated. Base: `c56d0502d7deb59ba85ac8a807c59a8d68729874`.

Issue #44 requests `gitcraft --diff A B` and `gitcraft --merge BASE LOCAL REMOTE MERGED`
outside repositories, using gitcraft's diff and three-pane conflict editor. Settings must
register gitcraft as Git's difftool and mergetool. This implements gitcraft as the invoked
tool; it does not implement invoking another external tool from a repository tab.

## Acceptance and boundaries

- Parse native `args_os` before constructing the normal application. Preserve arbitrary
  native file paths, including spaces, option-looking names and non-UTF8 Unix names.
  Support `--mergetool` as an alias for `--merge`, an optional positional `--`, and useful
  help/errors. Preserve normal repository opening and existing askpass/helper dispatch.
- Start a dedicated file-tool window with no repository discovery, profile fallback,
  background fetch, forge account lookup, update check or repository mutation actions.
- Read and compare immutable bounded raw bytes in a cancellable background worker.
  Reuse the existing read-only content-diff renderer and conflict choices/result editor.
  Text and binary/non-UTF8 comparisons must have explicit useful presentations; never
  invoke textconv, clean filters, SOPS, user hooks or a shell on file names/content.
- Build a text three-way merge with isolated `git merge-file --stdout --diff3` over
  application-owned constant temporary basenames, with inherited Git routing/configuration
  cleared. Keep BASE/LOCAL/REMOTE read-only. Show the base and the two side identities;
  retain conflict choices and an editable result. Binary/non-UTF8 merges offer explicit
  whole-side byte choices rather than lossy text conversion.
- Save only through an explicit Save and finish action. Guard the originally opened output
  bytes, file type and metadata; reject links/devices and inputs aliased as the output.
  Reserve app-owned sibling scratch/lock names and recheck before native atomic replacement.
  Preserve the displaced output in a private recovery location so concurrent writes cannot
  be silently discarded even across a replacement race. Drift must refuse success and show
  recovery locations. New outputs use create-only installation. Do not modify inputs,
  global configuration, an index or refs as part of standalone operation.
- Exit 0 only after a completed read-only comparison or a successful explicit merge save;
  cancellation/window-close of an unsaved merge exits 1; usage/load/runtime errors exit 2.
  Closing while a save owns its transaction must not abandon it or report false success.
- Settings show the exact intended Git integration and offer explicit registration.
  Configure tool-specific commands and `mergetool.gitcraft.trustExitCode=true`; preserve
  unrelated settings and report failures. Quote the executable and all Git placeholders
  correctly. Do not silently replace the user's default tool. Include copyable config and
  instructions for deliberate default selection and removal.

## Preserved implementation units

1. CLI dispatch, standalone data models and bounded native-file readers; isolated raw Git
   comparison/merge preparation and guarded output transaction, with fixture coverage.
2. Dedicated file-tool lifecycle/UI using the existing diff/conflict primitives, asynchronous
   ownership/cancellation and exact exit outcomes. Test actual UI choices, edits and cancel.
3. Explicit Git integration settings and docs, with fixture-only configuration checks.
4. Complete adversarial fixture coverage and private formatting, strict Clippy and full tests;
   bind logs/source tree to an immutable pushed candidate for independent review.

Each completed unit is committed under the repository's required Jonas Kamsker identity,
pushed and remote-SHA verified before the next unit or test. No Cargo slot is currently
authorized. Tests use owned temporary files/repositories only; no real merge, account change,
network write or new process selector/signal is part of development. The final consolidated
PR remains the integration owner's responsibility after all gates and independent review.
