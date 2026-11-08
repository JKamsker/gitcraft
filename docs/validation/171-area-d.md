# Issue 171: area D acceptance and platform notes

Scope: #171 checkboxes under items 9, 10, 12, 13. Original references read: #167,
#171 and #170; S14 (overlong branch/commit menu with duplicate working-tree comparison),
S15 (Open In ordering), S17 (empty Agents header). Original images are available to the
integration reviewer under `/tmp/gitcraft-167-171/reference-{14,15,17}.png`.

## Item 9 — context menus

- [x] Discoverable groups: clicked branch, Branches / Checkout, Compare, Rewrite,
  Patches, Links / Copy; optional History / Diagnostics. Local branch menus also group
  integration/comparison, review/versions, publishing/stack, branch actions,
  links/copy/history, visibility/agents. Custom-command scope is preserved.
- [x] Dedicated **Settings → Context menus** page with persisted switches for diagnostics,
  surgery, rehearsal/interaction, release. Defaults hide these niche tools. All remain
  in the command palette, including selected-commit surgery/tests/release/bisect and a
  branch picker for rehearsal. An active bisect remains accessible in the menu.
- [x] Disabled quick edits are inside Edit / Drop / Reorder. Existing shared-reason
  consolidation is retained (one reason per adjacent group), as are disabled tooltips
  for fold/surgery and guards/confirmations for rewriting/resetting.
- [x] Root and nested menus have viewport-derived height limits and scrolling. egui's
  Popup best-alignment and constrained Area flip/reposition at edges. Long forced-line
  conflict annotations can scroll horizontally too. Explicit submenu button scope IDs use the parent identity and branch/group label;
  scroll salts use commit hash/branch name/group, not row index or scroll offset.
- [x] Native/separate-viewport popup evaluated; unsupported fallback implemented (below).
- [x] WIP selection skips the redundant selected-row comparison. One working-tree
  comparison remains, with the clicked commit as base.
- [x] Toolkit menu buttons retain keyboard focus/activation, Escape closing and hover
  submenus; click scope, branch-ref identity and command confirmations remain intact.

### Outside-window popup evaluation

`egui::Popup` is an Area rendered into the current viewport. It cannot paint outside
that surface. This project has eframe immediate secondary viewports for document
pop-outs, but those are ordinary native top-level windows, not transient native menus.
Neither the app nor its egui/eframe integration exposes a native context-menu bridge.
X11 exposes `X11WindowType::PopupMenu` and override-redirect hints, but those flags do
not supply popup ownership or integrate egui menu input across windows. A document
viewport would need a second menu lifecycle, focus transfer, cross-window
hover/submenu handling, screen-position conversion and native transient ownership;
Wayland additionally does not permit arbitrary top-level window positioning. Merely
opening a document viewport would break the required hover, focus and close semantics.
A full native menu also cannot carry existing custom conflict/ghost hover content
through the current menu API. Therefore this implementation uses the explicitly allowed
unsupported-platform fallback on macOS, Windows and Linux, and embedded/headless hosts:
repositioned, height-limited scrolling menus. **It does not claim to extend outside the
application window.** Native outside-window behavior is not runtime-tested or enabled.

Source checked: egui 0.36.2 `containers/popup.rs` (`get_best_align`, `show`),
`containers/menu.rs` (`SubMenu`), `viewport.rs` (`with_position`, `with_window_type`),
egui-winit 0.36.2 native window creation and existing `src/repo/popout/window.rs`.

## Item 10 — Open In

- [x] Icons, consistent menu spacing, order: file manager → editor → Copy path →
  separator → web browser. Editor uses the code icon; copy uses copy; web uses
  external-link; file manager uses folder-open.
- [x] Existing SSH local-program restriction and shared explanation retained; Copy path
  uses the remote shell's path. Existing WSL/local mapping is unchanged. Missing web
  destination still explains why. The popup is bounded and scrollable too.

## Item 12 — optional Agents sidebar

- [x] **Settings → Agents → Show Agents section**, default off, persisted. Guard is
  before header registration, so the entire section (including a pinned header) disappears.
- [x] Separate from `agent_writes` (default and value unchanged); no task loading,
  execution, MCP permission or task data is altered.
- [x] Regression covers empty and populated task state while toggling, and settings
  serialization with either write-permission value.

## Item 13 — compact Appearance

- [x] Initial gallery contains the five GitCraft themes only.
- [x] Inline keyboard-accessible Show more / Show less reveals/collapses every other
  group (installed external, imported and editor themes stay available).
- [x] Current external theme is named in the collapsed disclosure without changing it.
  Existing total count, All/Dark/Light filtering, selection, active indication, hover
  preview and credit tooltips remain. Theme cards now have explicit accessible names
  and keyboard Enter/Space activation. A name search reveals matching external themes.
- [x] Other Appearance controls follow five cards instead of the entire external gallery.
- [x] Narrow settings use a page dropdown and optional preview disclosure, so desktop
  navigation/preview columns do not consume the theme gallery. Card widths adapt to
  available space. Width/scale interaction coverage is listed below.

## Validation

Commands use `cargo +1.95.0`, `CARGO_BUILD_JOBS=3` and the existing
`CARGO_TARGET_DIR=/Users/jonas/Documents/gitcraft/target`. No whole-suite run was started.

Focused tests named `area_d` cover menu duplicate/scope, edge placement, submenu hover,
scrolling to the last action, palette retention, Open In ordering/reasons, Agents state,
settings persistence, and Appearance disclosure/filter/selection/keyboard interaction.
The screenshot matrix uses 640/1100/1600 physical widths and 100/125/150% scale (logical
viewport divided by scale, rendered pixels-per-point set to scale). Screenshot generation
is opt-in through `GITCRAFT_AREA_D_SCREENSHOTS` to keep routine tests free of artifacts.

Runtime validation results and screenshot inventory are recorded below after execution.

### Executed results

- `cargo +1.95.0 check --lib --offline`: passed.
- `cargo +1.95.0 test --lib area_d --offline -- --test-threads=1`: **9 passed**.
  Re-ran the same 9 tests from an isolated copy of this worktree's test executable:
  **9 passed**. This avoids another agent replacing the executable in the shared cache.
- Related regressions run from that verified executable (its test list contains all nine
  `area_d` tests): `settings::tests` **15 passed**, `repo::palette::tests` **9 passed**,
  `repo::graph_view::` **39 passed** (including existing ghost/reload menu identity and
  reset safety tests). These counts overlap the focused tests; they are not additive.
- `cargo +1.95.0 fmt --all -- --check`: passed.
- `cargo +1.95.0 clippy --lib --tests --offline -- -D clippy::all`: passed. Existing
  platform-dependent dead-code warnings in desktop-entry/diagnostics code remain.
- `git diff --check`: passed.
- Visually inspected all 54 generated frames through six contact sheets, with individual
  checks of the narrow/high-scale menu, submenu, Appearance and settings pages.

### Screenshot inventory

Directory: `/tmp/gitcraft-167-171/area-d/` (shared local evidence, not committed binaries).

Each of these six prefixes has all nine combinations of widths `640`, `1100`, `1600`
and scale suffixes `1.00`, `1.25`, `1.50`, for **54 PNGs**:

- `menu-{width}-{scale}.png` — branch-pill + commit root near the bottom-right corner.
- `compare-{width}-{scale}.png` — hover submenu flips left, clear of sibling targets.
- `open-in-{width}-{scale}.png` — icons, order, spacing, unavailable web explanation.
- `appearance-{width}-{scale}.png` — initial five themes and disclosure, responsive page.
- `context-settings-{width}-{scale}.png` — optional-tools page, surgery toggled on.
- `agents-settings-{width}-{scale}.png` — independent Show Agents setting toggled on.

For a quick review, `contact-menu.jpg`, `contact-compare.jpg`, `contact-open-in.jpg`,
`contact-appearance.jpg`, `contact-context-settings.jpg` and `contact-agents-settings.jpg`
show each complete nine-frame matrix. Disclosure/selection/filter and Agents hidden/shown
states are exercised by the interaction tests; the PNGs capture the named states above.
At the smallest/highest-scale Appearance size, the last built-in card and controls below
are deliberately reached by page scrolling; the test scrolls to and operates them.

Logs: `/tmp/gitcraft-area-d-{check,tests,final-focused,fmt,clippy}.log` and
`/tmp/gitcraft-area-d-related-{settings,palette,graph}.log`.

### Remaining validation limits / integration notes

- Executed on macOS with headless egui/wgpu. Windows/Linux file-manager launching,
  WSL integration, screen-reader software and native desktop window behavior were not
  runtime-tested. Existing path-opening/WSL restrictions were preserved.
- Outside-window popups are unsupported by the current menu integration; the fully
  reachable fallback is used. This is the explicitly permitted limitation for item 9,
  not a claim of native outside-window support.
- Agent visibility tests use empty and populated task records; they do not start a live
  agent or alter MCP permissions. No full application suite was run in parallel.
- Shared integration touchpoints: `Settings` defaults/serde fields and `Page::ALL` gain a
  Context menus page; `sidebar/branches.rs` changes only branch-menu grouping and the
  optional advanced entries. Other agents' settings additions must be retained when
  combining these changes. No `graph_view/mod.rs` changes were necessary.


## Adversarial review follow-up

The three reported defects were reproduced by new interaction regressions before the
fixes: **0 passed, 3 failed** (`/tmp/gitcraft-area-d-followup-before.log`). No review was
delegated and no unrelated implementation areas were changed.

- [x] **P1 submenu action target after refs reload.** `menu_item::group` now gives the
  submenu **button** an explicit scope ID derived from its parent identity and label.
  A salted child UI alone still mixes sibling position into egui's automatic button ID.
  The regression opens Branches / Checkout → Branch bravo → Check out, holds the pointer
  over the action, inserts alpha before bravo, and clicks. It requires exactly
  `Checkout("bravo")`. Separate cases reorder and remove other refs. The pre-fix run
  dispatched `Checkout("alpha")`; all three cases pass with the fix.
- [x] **P2 expanded Preview at short/high-scale sizes.** The above-page preview is inside
  the same page scroll area as its controls, with a bounded preview scroller. This applies
  to both compact navigation and intermediate widths. At physical sizes 640×900, 800×500
  and 1100×500, each at 100/125/150%, the regression expands Preview, scrolls Show more
  into the viewport, asserts its full rectangle is inside the viewport, opens the other
  themes, scrolls to Catppuccin Latte, asserts its card is fully inside, and selects it.
  The pre-fix run failed at 800×500/125%; the nine cases now pass, including 800×500/150%.
- [x] **P2 explicit collapse during theme search.** Named external search matches reveal
  automatically only until the user chooses the disclosure state. An explicit collapse
  hides the cards even while the query still matches, and the button reads Show more.
  The regression searches Catppuccin, collapses, asserts the card is absent, reopens,
  verifies Show less and the card, and confirms the active theme was not changed.

### Follow-up validation and evidence

The test build compiled this worktree's source. Its executable was immediately copied to
`/tmp/gitcraft-area-d-followup-tests`; the captured inventory
`/tmp/gitcraft-area-d-followup-inventory.log` contains **all 12 area_d test names**, including
all three follow-up regressions. Related test runs use this isolated executable rather
than trusting shared-target freshness. The complete inventory is 3091 tests.

- Focused `area_d` run: **12 passed**, all names recorded in
  `/tmp/gitcraft-area-d-followup-tests.log`; copied-executable `area_d_followup` rerun:
  **3 passed**, `/tmp/gitcraft-area-d-followup-final.log`.
- Related copied-executable runs: settings **15 passed**, palette **9 passed**, graph
  **40 passed**; `/tmp/gitcraft-area-d-followup-{settings,palette,graph}.log`.
  Counts overlap the focused run and are not additive.
- `cargo +1.95.0 fmt --all -- --check` and `git diff --check`: passed.
- `cargo +1.95.0 clippy --lib --tests --offline -- -D clippy::all`: passed; the log
  confirms `Checking gitcraft` at this worktree. Existing platform-dependent dead-code
  warnings remain. Logs: `/tmp/gitcraft-area-d-followup-{fmt,clippy}.log`.
- The original six screenshot matrices were regenerated at widths 640/1100/1600 and
  100/125/150%, retaining all original area-D interaction coverage. No original acceptance
  checkbox above is deferred by this follow-up.
- New evidence directory: `/tmp/gitcraft-167-171/area-d-followup/`, **64 PNGs**: the original
  54 matrix frames, nine `followup-preview-{width}-{height}-{scale}.png` frames for the
  short-height matrix, and `followup-search-collapsed.png`. The screenshot is taken after
  scrolling Show more into view while Preview remains expanded above the visible range.
- Particularly relevant: `followup-preview-800-500-1.50.png`,
  `followup-search-collapsed.png`, and `contact-followup-preview.jpg`. Inspected the preview
  contact sheet, the minimum-window frame, the collapsed search, and regenerated menu and
  comparison contact sheets, plus the Appearance, Open In, Agents and Context menus
  contact sheets. Headless macOS/platform limitations above remain unchanged.
