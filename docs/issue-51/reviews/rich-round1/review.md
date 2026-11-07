# Batch B rich-content review R1 — FAIL

Candidate: **270a8a3b33402847f6d04cd8b8e70d625f33f4cb** (tree `1599b7a74b07a75d0bea17cfb0a7ffe86709e1ce`). Feature commits: `dde8f35544c5896d66c26ae4412bd611e5edd7d7` (dependency metadata), `270a8a3b33402847f6d04cd8b8e70d625f33f4cb` (rich Find). Both new ideas remain **FAIL / needs correction**. All findings below concern these features, not the six inherited setup failures.

Read-only review of Git objects archived to `/tmp/gitcraft-51-rich-r1-270a8a3`. No candidate source was read from the writer's moving checkout. No tracked edits, subagents, CI, full suite, broad application compilation, process-control tests, application binary execution, external registry requests, tokens, or live writes. The writer's `target/debug/deps` supplied cached third-party libraries for small source harnesses; no writer application/test binary was trusted or run.

## Requirements and evidence

Read the complete original [issue #40](https://github.com/JKamsker/gitcraft/issues/40), including comments, and `/tmp/gitcraft-51-audit-content.md` (baseline audit at `a850de26`). Relevant baseline gaps: visible/rendered Find; actual changelog rather than code-comparison links; Cargo licence enrichment with bounded, trustworthy fetching and explicit unavailable states. The user's review brief additionally requires rendered rich-type coverage, meaningful navigation, source ownership, and privacy/resource assessment.

Evidence is preserved in `/tmp/gitcraft-51-rich-r1-harness/`:

- `issue40.json`, the baseline audit, supplied validation JSON, and copies of its ten available logs.
- `source-hashes.json`: immutable candidate source hashes for extracted/included harness components.
- `build.py`, `main.rs`, `find.rs`, `archive.rs`, `build.log`, `results.log`, `r1-harness`.
- `state-build.py`, `state.rs`, `state-build.log`, `state-results.log`, `r1-state-harness`.
- `artifacts-sha256.json`: hashes of the preserved evidence files.

Independent execution: two small `rustc +1.95.0 --test` source harness builds succeeded. Six tests in `results.log` and one recording-only test in `state-results.log` succeeded **by asserting the defective behavior**. These are seven reproductions of failures, not a passing application gate. Existing Find engine/rendering modules and archive parsing were included from the frozen source; changed-prose helper and metadata State methods were copied unchanged. Application routing, theme setup and task/network infrastructure were stubbed or omitted as documented by the build scripts. Metadata State tests use synchronous recording-only fetch/task stubs and make no HTTP requests.

The writer's supplied ten targeted gates report exit 0, with the dependency gates bound to `dde8f355` and Find gates to `270a8a3`. The dependency implementation files are unchanged between those commits. These supplied results were inspected, not rerun; they do not cover the counterexamples below.

## Findings requiring correction

### R1-1 — P2: transient metadata failures become unretryable cached results

Source: `src/repo/diff/rich/dependency_metadata/mod.rs:195–205,249–258,274–278`.

`show()` puts every received `Answer`, including `Err`, into the cache. `select()` returns immediately for any cached result. Close/cancel clears the selection/result but leaves the cache. There is no retry, invalidation or error expiry. A temporary registry/proxy failure, cancellation or an initial offline lookup therefore poisons this package/version pair until cache eviction or destruction of the rich state. Going online and selecting it again cannot recover.

Reproduction: `r1_error_cache_and_side_short_circuit` executes the exact `State::select/show` with a recording fetch returning an error, then changes the fetch to succeed and reselects the same package. The result remains `Err`; the recorded calls do not increase. Evidence class: executed recording-only source harness. Infrastructure timing/cancellation itself was not exercised.

Correction: allow explicit retry or avoid retaining transient errors indefinitely; test recovery through the normal selection/close flow.

### R1-2 — P2: failure on either dependency side hides available metadata on the other

Source: `src/repo/diff/rich/dependency_metadata/mod.rs:217–222,285–289`.

The worker constructs one `Result<(old,new)>` with `?` on both fetches. If Before fails, After is never attempted. If After fails, already-read Before metadata is discarded. Thus an available exact updated package's changelog/licence disappears merely because its predecessor cannot be downloaded or parsed, including when the predecessor exceeds a bound. The panel only shows a single error.

Reproduction: the same recording test requests `(old,new)`, makes the old fetch fail and permits the new fetch to succeed. Recorded calls are exactly `["old"]`; new is never attempted. Evidence class: executed recording-only source harness plus exact source control flow for the opposite direction.

Correction: retain independent per-side results; show each available side and its provenance, and clearly report the failed/unknown side without inferring a licence change.

### R1-3 — P2: published Cargo `license-file` is discarded and represented as no declaration

Source: `src/repo/diff/rich/dependency_metadata/archive.rs:73–78`; `dependency_metadata/mod.rs:131–135,149–152,290–302`.

The archive reader only keeps the manifest and changelog-like filenames. Cargo parsing only reads `package.license`; it ignores `package.license-file` and never retains its content. A package explicitly declaring `license-file='LICENSE'` and shipping that file yields `license=None`; the panel then says “not declared in published manifest.” Changes to the declared licence file cannot be displayed or compared. This is an available licence source, not a missing declaration. Legacy npm licence objects are also ignored by the new parser, although the existing lockfile parser recognizes that representation.

Reproduction: `r1_license_file_is_discarded` supplies a valid bounded package with exact name/version, `license-file='LICENSE'`, LICENSE content and CHANGELOG.md. Parsing succeeds with `license=None`, retaining only the changelog. Evidence class: executed exact archive/parser source harness. No real registry package was fetched.

Correction: represent and safely retain declared licence-file information/content, distinguish a file declaration from missing metadata, and preserve unknown states rather than silently comparing missing strings as complete licence information.

### R1-4 — P2: size-filtered changelogs are falsely reported as absent from the package

Source: `src/repo/diff/rich/dependency_metadata/archive.rs:67–78,92`; `dependency_metadata/mod.rs:313`.

A changelog exceeding 512 KiB is silently skipped by the archive match guard. Parsing then succeeds with an empty changelog map, and the panel asserts “not included in the published package.” The file is included; the reader refused it. Keeping the bound is sensible, but the state and explanation are incorrect. Unrecognized changelog filenames are likewise indistinguishable from absence.

Reproduction: `r1_large_changelog_is_silently_absent` supplies a 524,289-byte CHANGELOG.md plus a small valid Cargo manifest. Parsing returns success and an empty map. Evidence class: executed exact archive/parser source harness plus UI message source.

Correction: keep explicit skipped/unsupported/oversized evidence and report it, or provide bounded excerpts with a clear truncation notice. Do not assert archive-wide absence after filtering.

### R1-5 — P2: visual row splitting changes Find's logical matching semantics

Source: `src/repo/find/rendered.rs:14–48,203–208`; `src/repo/find/scan.rs:94–109`.

Every logical row is physically split at 5,000 characters, and the engine searches the resulting fragments independently. A real match spanning the cut disappears. The artificial boundary also creates false whole-word/regex line boundaries. Retaining all characters is insufficient when logical positions and boundary context are lost.

Reproductions through the actual Find engine and headless reading renderer:

- `r1_boundary_loses_match`: `"x"*4997 + "needle" + "x"*10` contains `needle` across characters 4997..5003, but Find reports 0 matches.
- `r1_boundary_invents_whole_word`: `"x"*5000 + "needle"` has no word boundary before `needle`, but whole-word Find reports 1 match.

Evidence class: executed source/headless harness with original matcher, scanner, ordering, mark painting and reading renderer. Correction: search logical text with correct context and map hit ranges into display segments; do not treat visual chunks as independent logical lines.

### R1-6 — P2: changed-prose search drops the paragraph tail before scanning

Source: `src/repo/diff/rich/search.rs:19–20`; `src/repo/diff/rich/prose_view.rs:326–334`.

Kept/added/removed prose feeds complete text into the reader, but Changed prose calls the ordinary display `part_job`, whose `changed()` helper stops after roughly 20,000 bytes. The reading document never receives the remaining words. The resulting Find behavior depends on whether the same content was classified as changed or added, and the omitted suffix cannot be searched. `bound_rows()` cannot restore data already discarded by the display helper.

Reproduction: `r1_changed_prose_tail_is_missing` supplies 11,000 kept `"a "` words followed by added `TAIL_NEEDLE`. The exact changed-prose helper returns 20,006 characters without the needle from a 22,011-character input. Evidence class: executed extracted-helper source harness plus exact rich-search dispatch. The ordinary rich display's preexisting truncation is not a new defect by itself; reusing that truncated output as the new search source is the defect.

Correction: build complete semantic styled text for searching, applying display bounds only after preserving logical text/ranges, or explicitly provide supported access to omitted content instead of a complete-looking search result.

### R1-7 — P2: meaningful displayed rich content is absent from the reading source

Source: `src/repo/diff/view.rs:197–205`; `src/repo/diff/rich/search.rs:50–73,95–109`; `src/repo/diff/rich/deps_view.rs:54`; `dependency_metadata/mod.rs:289–314`; `src/repo/diff/rich/dvc_view.rs:102–104`.

The dependency reading document accepts only `RichDiff`; it cannot access `RichState.metadata`, where the newly fetched published licences and changelogs live. Open a package's actual changelog/licence panel and then Ctrl+F for a distinctive release-note phrase or a Cargo licence enriched from the archive: the whole panel disappears and none of that text is in the search document. Cargo lockfiles ordinarily do not provide the enriched licence string in `DepChange.license`, so even the newly obtained licence can be unfindable. Metadata pending results also are not polled by that pane while Find replaces it.

A second concrete omission is DVC folder file counts: the normal view displays `a folder of {files} files`, but the reading builder omits `Tracked.files` entirely. Searching that meaningful number/text removes the displayed information and cannot locate it.

Evidence class: exact source/UI-route proof, not a full-app interactive repro. This is not an objection to the alternate reading layout: that layout must preserve the meaningful text it substitutes for. Correction: include the currently available metadata and full DVC content in the reading model, with source/version ownership and invalidation when async metadata arrives.

### R1-8 — P2: Markdown cache identity omits the rendered-content generation

Source: `src/repo/diff/view.rs:108–110`; `src/repo/find/rendered.rs:183–188`; `src/repo/navigate.rs:268–294`; `src/repo/state.rs:265–278`.

Markdown async loading uses `(view.request,view.rendered_load)` ownership, and toggling rendered mode deliberately reloads it without changing `view.request`. The Find document identity includes request but omits `rendered_load`. After a successful Markdown reread under the same request, a still-open Find can retain the old text and hits because `Find::rendered()` skips its builder for that identity. Rich and Syntax callers already include their corresponding generations.

Reproduction: `r1_cached_document_ignores_replacement` renders `old`, then supplies a builder for `new` under the same Markdown identity. The second headless output still paints `old`. Combined with the exact application caller/loader above, this establishes the missing-generation route. Evidence class: executed source/headless cache repro plus application source ownership proof; no filesystem-watcher race or full UI toggle was run.

Correction: include `rendered_load` (or an immutable content revision) in Markdown identity and test a new accepted rendered result with unchanged request while Find stays open.

## Full coverage assessment

| Area | Assessment at the reviewed SHA |
|---|---|
| Actual dependency changelog | Reads exact published archive contents and shows archive links; substantive implementation, not a guessed repository/changelog URL. Fails partial-error handling, retry and filtered-file explanation (R1-1/2/4). |
| Actual dependency licence | Exact manifest identity checks and string licence comparison work by source and supplied fixtures. Declared licence files are lost (R1-3). Missing/unsupported states otherwise have explicit wording; no blanket “safe” classification. |
| Package/source authority and privacy | Cargo public-source allowlist and npm public-resolved-source checks gate metadata requests; unsupported/private-only requests return locally. Archive manifests must match requested name/version. npm manifest and tarball host are checked; redirects disabled. No repository scripts, shell execution, filesystem extraction, registry auth or publishing in this path. Lockfile checksum/integrity is not verified; provenance is trusted public-registry name/version, not cryptographic proof against the lockfile bytes. No exploit of that distinction was established here. |
| Networking, async ownership, cache | Opt-in worker, offline check, finite timeout/body/expanded/text/count bounds, task scope, selected-receiver replacement, pending profile-context guard and eight-entry memory cache exist. Concrete error-cache and per-side defects are R1-1/2. Completed cache keys have no profile/offline epoch, but payloads are public metadata; no private cross-profile payload leak was demonstrated. |
| Markdown | Inline visible words are joined across emphasis/link labels; link destinations are omitted. Same parser options as ordinary viewer; raw HTML is also shown as text by the ordinary installed renderer, so its inclusion is not treated as a markup leak. Hits are highlighted and stepped in the reader. Boundary and generation failures R1-5/8 remain. Footnote references are omitted by the new event match; this is a smaller visible-text parity limitation. |
| Prose | Kept/added/removed text and folded paragraphs are exposed in reader; changed text retains inline difference styling but is prematurely truncated (R1-6). |
| Config and API | Key/value previews, type changes, endpoints, severity and descriptions are represented. No hidden raw patch is substituted. Config previews retain the existing model's 80-character display cap; this is search of displayed previews, not proof of full-value search. |
| Dependencies | Names, versions, change classification and lockfile licence deltas represented; fetched details omitted (R1-7). |
| Tables | Both sides' full row values and column names represented, including folded rows. Common engine boundary defect R1-5 still applies to long rows. |
| DVC | Paths, size, full hashes and source repository represented; folder counts omitted (R1-7). |
| Notebooks | All comparison cells' sources, text/stream/error outputs and cell/notebook metadata represented; image outputs explicitly described as nonsearchable pixels. Before/After and cell labels give reader context. Existing output parser caps at 200 lines with a notice; text beyond extraction is not recoverable by this search. With JSON selected, opening Find still forces the semantic cell reader because interception precedes the Cells/JSON switch; this is a scope/interaction limitation, not evidence of raw JSON search parity. No notebook execution added. |
| Binary documents / archives | Extracted text, extraction gaps/errors, facts/sizes, archive member names/sizes, and bounded hex/ASCII head included; useful explicit coverage. Nothing searches unextracted bytes beyond existing bounds. |
| Syntax | Both sides' structural rows, source line numbers and change headings represented; generation included; staging remains in patch mode. Source model supplies only structural rows, so this does not claim all omitted unchanged file text is searchable. |
| LFS / images | LFS size/OID text included. Images explicitly say pixels have no searchable text. No OCR claim or hidden patch-result claim. |
| File / blame and controls | Existing file/blame search retained. Ctrl+F routing now reaches rich views; existing Enter/Shift+Enter/F3 handling and actual common mark painter are used. Supplied targeted gates test these, and the independent harness exercised actual engine/renderer. No full-app focus/keyboard integration rerun. |
| Encrypted data | No SOPS/decryption implementation is present in this fixed candidate; no decrypted data was generated or cached in review. Therefore no plaintext-cache guarantee for a later encrypted feature can be inferred. New Find cache is memory-only and cleared by close; it must not be reused for later decrypted content without an explicit ownership/lifetime design. |

## Selection, navigation and resource limits

The alternate reading view is accepted as a design choice. Match stepping and marks are real, not a dummy counter, and Before/After/cell/source-line labels provide useful context. However `Document` stores only layout jobs and the reader allocates `Sense::hover()` rows. It has no text selection/copy interaction, clickable rich source action, or logical anchor back to the original rich row. `Find::close()` clears the current hit, and original rich scroll areas have different IDs, so locating a distant config/API/table row then closing Find does not transfer that location for its normal action. These are concrete functional limitations of the current design, not styling objections; this review did not independently gate the requested highlight/step behavior solely on adding selection or source-jump functionality.

Search scanning has a 4 ms/frame budget and hit cap, and visible-row painting is virtualized. Initial `build()` and `bound_rows()` nevertheless construct/copy the entire document synchronously on the UI thread before that budget applies, even for an empty query. Rich input allows up to 8 MiB per side; many short lines can create millions of separately allocated layout jobs. No near-limit memory/latency stress was run under this review's bounded-fixture constraint. This is a source-established resource-design concern; it is not a claimed measured OOM or a ninth reproduced blocker. Changelog display similarly wraps retained text on the UI thread and has no independent latency measurement here.

The independent harness stubs task scheduling/theme/application routing and cannot certify platform UI behavior, real proxy/registry interoperability, cancellation timing or complete application gates. No source edits or fixes were made. Correct these findings on a new preserved commit, rerun the relevant focused checks, and obtain a new independent review of that exact successor; this report remains bound exclusively to `270a8a3b33402847f6d04cd8b8e70d625f33f4cb`.
