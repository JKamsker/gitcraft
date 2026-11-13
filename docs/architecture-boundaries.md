# Architecture boundaries and fitness checks

Issue [#209](https://github.com/JKamsker/gitcraft/issues/209). The file-size and
cognitive-complexity gates ([#208](https://github.com/JKamsker/gitcraft/issues/208)) keep
files and functions small; they say nothing about *which code depends on which*. This gate
makes dependency direction, feature ownership and `RepoTab` state access observable, and
fails CI when a change makes them worse. It does not ask for a rewrite: every coupling that
existed when it was adopted is recorded in a reviewed baseline that can only shrink.

- Policy (versioned, reviewed): [`ci/arch-fitness/policy.json`](../ci/arch-fitness/policy.json)
- Baseline (legacy exceptions and budgets): [`ci/arch-fitness/baseline.json`](../ci/arch-fitness/baseline.json)
- Checker: [`ci/arch-fitness`](../ci/arch-fitness) (a `syn`-based Rust tool, its own workspace)
- Report at adoption: [architecture-fitness-report.md](architecture-fitness-report.md)

## Running it locally

From the repository root, with Rust 1.95+:

```sh
# The check CI runs, against the committed candidate (raw Git blobs of HEAD):
cargo run --release --manifest-path ci/arch-fitness/Cargo.toml -- check --revision HEAD
# The same for the working tree (tracked files only; `git add -N` new files first):
cargo run --release --manifest-path ci/arch-fitness/Cargo.toml -- check
# Reports: deterministic JSON, Markdown and a Graphviz domain graph.
cargo run --release --manifest-path ci/arch-fitness/Cargo.toml -- report \
  --json arch.json --markdown arch.md --dot arch.dot
# The checker's own fixtures and negative controls:
cargo test --manifest-path ci/arch-fitness/Cargo.toml
```

`report` never fails; `check` exits 1 on a violation and 2 when it cannot run (bad policy,
not a Git repository). Output is byte-identical for the same tree and policy: no timestamps,
machine paths or hash-map order. In CI the shared `rust-file-size` gate (required by PR,
mainline and release workflows) runs the tests and `check --revision HEAD`, and puts the
Markdown report in the job summary. The analysis reads every `cfg` branch of every file, so
the Linux run also covers Windows- and macOS-only code; no separate Windows run is needed.

## Understanding a failure

Each violation names the rule, the offending edge or field, the first source location and
symbol, and the remediation, e.g.:

```text
[forbidden-edge] new dependency alpha (features) -> beta (features): 1 references to 1 symbols; this direction is not allowed
  at: src/alpha.rs:12
  symbol: gitcraft::beta::go (written `crate::beta::go`)
  fix: depend on a lower layer instead (move the shared piece down, or pass a callback/event/Action up); ...
```

| Rule | Fails when | Usual fix |
| --- | --- | --- |
| `forbidden-edge` | a domain starts depending on a domain the policy does not allow (upward, or feature → feature) and the edge is not in the baseline | move the shared piece to a lower layer, pass a callback/event, route through `repo-core` with an `Action`; or add a reviewed `allowed_edges` entry |
| `domain-cycle` | domains form a new strongly connected component | remove the edge that closed the loop (usually the upward one) |
| `cycle-edge` | a new edge appears between domains of an existing (baselined) cycle | as above: do not deepen a known cycle |
| `repo-tab-mutation` | a domain writes, mutably borrows or calls a mutating method on a `RepoTab` field it does not own, and is not a coordinator | add a method to the owning feature, or return an `Action`/effect for the coordinator; if ownership really moved, change `repo_tab.owners` |
| `visibility-reexport` | a module re-exports (`pub use`, `pub(crate) use`, ...) another domain's item | import from the owner where it is used; keep the `use` private |
| `visibility-budget` | a domain exposes more items beyond its boundary (effective visibility) than its budget | narrow to private, `pub(super)` or `pub(in <domain root>)`; the message lists exported items nobody outside uses |
| `fan-out-budget` | a domain depends on more domains (or, for `symbol_budget_domains`, more foreign symbols) than its budget | use the APIs already depended on; the message lists targets and single-use symbols |
| `unmapped-module` | a module belongs to no domain | add it to a domain in `policy.json` (new `repo` submodules must be classified explicitly) |
| `analysis-gap` | the inventory is incomplete: a parse error, missing or ambiguous module file, a tracked `.rs` file no crate reaches, or `RepoTab` not found | add the crate root to `crates`, or list the file in `ignored_files` with a reason |
| `stale-baseline` | an exception no longer occurs | run `ratchet` and commit the smaller baseline |

## The baseline and ratcheting

Every exception records `reason`, `owner` and `remediation`. The initial entries are the
coupling measured at adoption; their reasons name an example site, so a reviewer can find it.

- **CI never writes the baseline.** `ratchet` and `adopt` refuse to run when `CI`,
  `GITHUB_ACTIONS` or `TF_BUILD` is set, and the CI step's exact command is pinned by
  `ci/test_workflow_gates.py`.
- `ratchet` only removes exceptions that no longer occur and lowers budgets to the measured
  values. A stale exception fails `check`, so a removed coupling cannot silently return.
- `adopt --reason .. --owner .. --remediation ..` records every current violation and raises
  budgets. It exists for adoption and for an exception a reviewer explicitly accepted;
  prefer fixing the violation, and replace the generic reason with a specific one.
- Budgets are not global magic numbers: each is the domain's own measurement. `check`
  prints budgets that can be lowered.

## Layers and allowed directions

Domains are grouped into layers, lowest first. A domain may depend on any domain of a lower
layer. Inside a layer, dependencies are allowed only where the layer says `peers`.

| Layer | Peers | Contents |
| --- | --- | --- |
| platform | yes | Operating system, processes, workers and file access; no product concepts. |
| presentation | yes | Reusable presentation without repository state. |
| services | yes | Git, forges, remotes and settings: capabilities features use; no feature UI. |
| features | **no** | Product features. A feature may not depend on another feature unless an allowed edge says so. |
| coordinator | no | `RepoTab`: composes feature state and dispatches `Action`s. |
| shell | yes | The application window, entry points and dev harnesses. |

Declared exception (`allowed_edges`): every feature may use `repo-core`. Features extend the
coordinator with `impl RepoTab` blocks and use its `Action`, `Msg`, notice and dialog types
and repo-wide helpers. These edges are excluded from cycle analysis (`"cycles": false`) because
they are the coordinator's composition pattern, not accidental loops; field *writes* are still
governed by the `RepoTab` rules below. Cycles are computed over every other edge, so a
feature ⇄ feature or service ⇄ feature loop is still found.

## Domains

`policy.json` is authoritative; this table summarizes version `2026-11-13.1`. A module
belongs to the domain with the longest matching prefix (`gitcraft::repo::diff` covers its
submodules); a pattern starting with `=` matches only that module, so `=gitcraft::repo` keeps
new `repo` submodules unmapped until someone classifies them. Crates without patterns
(the binary, `gitcraft-os`, tests, examples, build scripts, the recovery harness and this
checker) take their crate's `domain`.

| Domain | Layer | Owns |
| --- | --- | --- |
| `os` | platform | The gitcraft-os crate and OS shell integration: taskbar, single instance, desktop/Finder/Nautilus entries. |
| `runtime` | platform | Process event loop, bounded workers, timing, safe file reads, PATH, network policy, portable mode, catalogs, sound, diagnostics and secret scrubbing. |
| `ui-kit` | presentation | Shared presentation: theme tokens, icons, widgets without product state, accessibility, key list. |
| `forge` | services | Forge APIs (GitHub, GitLab, Forgejo, Gitea, Gerrit), pull requests, CI runs, issue trackers, avatars, CODEOWNERS. |
| `git` | services | Typed Git operations, graph lane layout and merge drivers. |
| `remote` | services | Repositories on other machines (WSL, SSH) and connection profiles. |
| `settings` | services | User settings, the settings window, the portable setup file and persisted drafts. |
| `agents` | features | Coding agents: worktree tasks, the MCP server, AI commit messages, authorship, collisions, drift and the agent fence. |
| `bisect` | features | Bisect, parallel bisect, the test lane, interaction finding and local CI. |
| `changes` | features | The commit detail pane: working tree, staging, the commit box, partial stashes, LFS, Unity, recovery. |
| `conflicts` | features | Conflict editing, conflict prediction, agent resolution, remerge diffs and rehearsals. |
| `diff` | features | Diffs, file views and comparisons, find, folding, highlighting, line dossiers, symbol history, weakened checks. |
| `doctor` | features | The repository doctor, repository tools, cleanup, error cards and the activity log. |
| `graph` | features | The commit graph and its history reading, ordering, filters, search, saved views and bands. |
| `guidance` | features | Practice repositories, the first-day tour, guidance and celebrations. |
| `insights` | features | Insights views, the folder atlas and file-tree growth. |
| `release` | features | Releases, hotfixes, package previews, range-diff versions and postcards. |
| `review` | features | Pull requests, reviews and CI in the tab, forge links, forks, DCO, issues and CODEOWNERS views. |
| `rewrite` | features | History rewriting: interactive rebase, surgery, stacks, fold, quick edit, drag and drop, ghost runs, timeline/undo, reflog, patches. |
| `sidebar` | features | The sidebar: branches as a tree, their order, health and cleanup, pinned sections. |
| `sync` | features | Push, pull and their preflight, sign-in recovery, SSH setup, offline mode and remote watching. |
| `terminal` | features | The integrated terminal, custom commands and per-worktree environments. |
| `update` | features | New versions of gitcraft. |
| `worktrees` | features | Worktree tabs, teleport and cross-repository views. |
| `repo-core` | coordinator | RepoTab: composition, dispatch of Actions, background messages, lifecycle, loading, navigation, operations, dialogs, palette, layout. |
| `app` | shell | The window shell, entry points (CLI, MCP stdio host, tray, standalone tools), manual and the gitcraft binary. |
| `dev` | shell | Integration tests, examples, build scripts and the recovery-recording harness (test-only). |

Sibling modules are not separate domains: a domain is a feature with its state, views and
background work, e.g. `terminal` is `crate::terminal`, `crate::custom_commands` and the tab's
`repo::terminals`, `repo::custom_commands` and `repo::devcontainer`.

## State ownership: `RepoTab`

`RepoTab` is the legitimate central coordinator and its field count is **not** a finding.
What matters is who changes which field:

- A field belongs to the domain that defines its type when that domain is a feature
  (`terminals: terminals::Terminals` belongs to `terminal`; `dossier: line_dossier::State`
  to `diff`). `repo_tab.owners` can name an owner explicitly. Everything else (`status`,
  `dialog`, `busy_op`, `selected`, the loaded history, ...) belongs to `repo-core`.
- `repo-core` is a coordinator and may change any field (orchestration).
- Any other domain changing a field it does not own is a *cross-feature mutation*.
  Construction (struct literals) and test code are reported but not enforced.

Access modes: `read`; `write` (`=`, `+=`, ... including nested places like `tab.f.x = ..`);
`mut_borrow` (`&mut tab.f`); `mut_call` (the field is the receiver of a method that mutates:
a crate method taking `&mut self` more often than `&self`, or a std collection/`Option`/cell
mutator); `init` (struct literal). At adoption, 138 fields belong to `repo-core` and 108 to
16 features; 238 (domain, field) pairs are cross-feature mutations. The hottest fields are
`status`, `busy_op`, `dialog`, `diff`, `file_view` and `ci_view`, each changed by 10+ domains;
`review`, `changes` and `rewrite` mutate the most foreign fields.

## How the inventory is built

1. **Crates** come from `policy.json`: the library, the binary, `gitcraft-os`, tests,
   examples, build scripts, the recovery-recording harness and this checker. Every tracked
   `.rs` file must be reached by one of them or be listed in `ignored_files` with a reason.
2. **Module tree** (`syn`): file modules (`x.rs`, `x/mod.rs`), inline modules, `#[path]` (in
   files and inside inline modules, with rustc's directory rules) and `include!("..")`.
   Every `cfg` branch is kept; only predicates a crate fixes (`recovery_recording`) are
   evaluated, three-valued, so only provably inactive items are skipped.
3. **Names**: items per name space, `use` trees (aliases, `self`, groups, globs, `pub use`),
   `extern crate`, `crate`/`self`/`super`/`Self` and uniform paths. Glob imports honour
   privacy; re-exports resolve to the defining item, so an alias, wrapper module or re-export
   does not hide the real dependency (the re-exporting module is reported separately).
4. **References**: every path in types, expressions, patterns, trait bounds, `impl` headers,
   block-level `use`, and macro bodies that parse as expressions (`format!`, `assert!`,
   `matches!`, `vec!`, ...). Locals, closure/match/`for`/`let` bindings and generic
   parameters shadow module names. Each reference is attributed to the module that contains
   it, so moving a call into a helper keeps the edge in the helper's domain.
5. **Domains**: module → domain mapping; edges, SCCs, fan-in/out, foreign symbols, effective
   visibility (an item's own visibility narrowed by its enclosing modules), and
   cross-domain re-exports. Test-only code (`cfg(test)`, `#[test]`, test crates) is listed
   separately as `test_edges` and not enforced (`enforce_tests` in the policy).
6. **`RepoTab` fields**: accesses through `self` in `impl RepoTab` (however the type is
   spelled or aliased), parameters/closure parameters/`let` bindings typed as the tab, and
   bindings initialised from a tab constructor or a function every definition of which
   returns the tab.

## Coverage and known gaps

The report's *Analysis gaps* section counts each of these; none is silently dropped.

- **Method calls are not resolved** (no type inference): `x.method()` creates no edge. The
  type, path or `use` that brings the receiver's type into scope usually does. Dynamic
  dispatch and trait objects are likewise invisible beyond their trait's path.
- **Field receivers of unknown type**: `tab.field` where `tab` comes from an untyped closure
  parameter, an iterator or another struct's field is counted as *unattributed* (per file),
  not attributed to a field. Most are other structs' fields of the same name.
- **Macros**: `macro_rules!` definitions and bodies that are not expression lists (`json!`,
  `thread_local!`, `select!`, DSLs) are not analyzed; item-level macro invocations and
  unparsed bodies are counted by name. Derives and attribute macros are not expanded.
- **Generated code**: `include!(concat!(env!("OUT_DIR"), ..))` (the i18n catalogs) is an
  `include-unresolved` gap.
- **Paths that do not resolve** (31 at adoption: macro-generated statics, associated
  types such as `Self::Child`) are listed with their sites as `unresolved-path`.
- **cfg**: all branches are analyzed together; a reference compiled only on Windows is
  enforced like any other, and its sample shows the `cfg`. Feature flags are not evaluated.
- **Shared files**: the recovery harness compiles production files through `#[path]`; these
  appear as `file-reached-twice` and belong to each crate separately.
- **Function-local items** are treated as locals of the enclosing module.

Fixtures (`ci/arch-fitness/tests/resolution.rs`) cover qualified paths, `use` aliases,
module aliases, block-level imports, type aliases, helper indirection (inside a domain and
moved into a shared one), `pub use` and glob re-exports, glob privacy, `#[path]` in files and
inline modules, cfg alternatives and test-only code, impl headers, macro bodies, `include!`,
local shadowing, gaps and determinism. Negative controls (`tests/rules.rs`) show each rule
failing on a deliberate violation and passing once it is reverted, plus stale entries,
ratchet monotonicity and the CI refusal.

## Using the report to prioritize

The report is an index for targeted refactors, not a to-do list:

- *Forbidden-by-policy edges* from platform/presentation/services into features (for
  example `git` → `app` through `standalone::access`, `os` → `agents` through the MCP wire
  types, `settings` → feature settings pages) are the edges that hold the single large cycle
  together. Removing them one at a time splits it; `ratchet` records the progress.
- *Modules with the most foreign dependencies* (`repo::tab_model`, `repo::actions`) are
  composition points; growth there is expected to be deliberate.
- *Cross-feature mutations* show which coordinator fields features write directly
  (`status`, `busy_op`, `dialog`): candidates for narrow coordinator methods or effects.
- *Exported but unused* items per domain are visibility that can be narrowed for free.
