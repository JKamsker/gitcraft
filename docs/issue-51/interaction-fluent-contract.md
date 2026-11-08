# Fluent migration contract and ownership

Status: **foundation WIP**, not full #47 localization completion. The pre-Fluent
application checkpoint is `dda1f66169bad0c937f4d8472ad26842d89ad414` on this branch;
root's separate assembly supplies the newer combined source for migration. Do not
migrate only this branch's older feature inventory.

## Shared API (owned by interaction writer)

- `crate::i18n::text("stable-message-id") -> &'static str`: no-argument label.
  Values are interned once per embedded catalog; existing static UI interfaces work.
- `crate::i18n::message("id", &[("name", value.into())]) -> String`: complete
  sentences, interpolated values and Fluent plurals. `Value` reexports `FluentValue`.
  Pass counts as numeric values, not English strings. Use named arguments instead
  of translating a Rust `format!` template or assembling translated fragments.
- `number(value) -> String`, `date_iso(&str) -> String`: English/German grouping,
  decimal separators and civil dates. Invalid ISO dates are preserved. Git/storage
  date formats stay unchanged; only presentation uses these functions.
- `language() -> Language` (`English`/`German`), `generation() -> u64`: language
  cache ownership. `set_language(&str) -> bool` is the settings adapter; application
  code normally reads the API rather than mutating selection.

`Settings.language` persists an explicit locale or empty for OS preferences. Supported
parent languages select English (`en-US`) or German (`de`); unknown explicit values
are preserved and fall back to English. Empty selection tries OS preferences in order,
then English. Native preference discovery is cached; headless tests use English.
Missing/invalid selected messages retry English; unknown IDs remain visible as IDs.
The engine uses concurrent Fluent bundles for deferred UI and background presentation.
This foundation supports these two LTR languages; no RTL layout or other language is
claimed. Launcher and palette presentation caches include the locale generation.

## Disjoint catalog ownership

Each migration writer owns paired `locales/en-US/<partition>.ftl` and
`locales/de/<partition>.ftl` files and their assigned source paths. Use a unique
partition prefix for every message ID (for example `repo-core-diff-open`). English
and German files must contain the same IDs and named variables. Provide genuine
German translation. Preserve Git terms/identifiers where their literal spelling is
part of the action. Avoid message attributes/terms initially; top-level messages are
supported by both the static-label cache and interpolation API.

The build discovers sorted `*.ftl` files automatically, so adding a disjoint catalog
requires no shared Rust index edit. Generated indexes live only in Cargo OUT_DIR.
Do not edit foundation.ftl, i18n modules, build.rs, Cargo files, settings wiring, or
another partition's catalogs. Request a narrow shared-interface change from this
writer if a real source constraint requires one.

Translate application-owned labels, titles, help/tooltips, accessibility labels,
notifications, errors and manual tables. Preserve stable widget/action/session IDs,
argv/options, protocol/JSON/config keys, regexes, paths, refs, user/repository/provider
content, source examples and diagnostic payloads. Wrap an external error in a
translated sentence with `$detail` rather than translating the external payload.
Never use translated text as an identity. Const tables should store stable message
IDs and resolve them when presented; do not call runtime translation in a const.

## Coverage and delivery

Use the exact combined pre-Fluent assembly tip for the source inventory and worker
base. The earlier d7db lexical inventory is only a sizing aid: it deliberately includes
false positives such as argv/protocol strings and requires source classification.
Each writer reports all assigned source paths as migrated, intentionally literal
(with reason), or still pending. Include source/catalog hashes and direct pure or
headless checks of representative real presentation adapters, interpolation/plurals
and unchanged literal commands. Passing catalog parsing alone is insufficient.

Commit/push/verify each coherent worker unit with the required author. Keep generated
logs, adapters, receipts and binaries outside the repository; only concise text/JSON
source records belong here. All process/native/network runtime holds remain in force.
Root aggregates the accepted partitions and builds one final Fluent feature commit.
The interaction writer retains settings/fallback/catalog aggregation and original
interaction behavior ownership; root alone allocates implementation workers.

## Foundation checks and limits

Writer checks: five selected Fluent pure groups (real embedded parser/duplicates,
ID/variable parity, language negotiation, English fallback, real plural/number/date
rendering, saved setting compatibility and actual launcher labels), one headless
actual language-control selection test, and macOS all-target compilation. The
headless control switches to Deutsch and back to English and asserts the actual
localized format preview. Its ComboBox exposes a descriptive accessible name and
current selected value. Exact immutable commands/log/source hashes are retained at
`/tmp/gitcraft-interaction-fluent-foundation-validation.json` after preservation.
No OS locale discovery, native menu installation, process/network fixture, Linux or
Windows execution, full suite, strict/qualified lint green or CI is claimed.
Only the foundation's labels are migrated here. All remaining UI/manual/error
catalog migration and final combined completeness checks are explicitly pending.
