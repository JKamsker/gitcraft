# Independent fileviews SVG R4 correction review

**PASS within this scoped review on exact parent `131cd833ea9504daec87400ec8efcacd583cfe7a`.** FV-R3-1, compact clip-path DAG render amplification, is closed by refusing clipPath definitions before usvg. The prior three corrections and the dependency merge are preserved. This does not approve the final combined application or clear the process/runtime/full-suite hold.

## Immutable binding and preservation

Read the complete `/tmp/gitcraft-51-fileviews-review-r3.md`; reviewed immutable objects exported to `/private/tmp/gitcraft-fileviews-svg-r4-131cd833`. The exact correction changes only `src/svg.rs` from merge `916c13ee9909d5a633a8dd0e0cbe5dcc689b835b`: one refused tag, explanatory comments and one bounded regression function. No moving application source was used.

The merge parents are `32ea63a91a0aebae3183d1822c4bdacd1c39914b` and preserved fileviews docs tip `9fd9b235051d2f5a3e244aa1cb641630c24ba896`. Before the correction, merge SVG bytes exactly equal reviewed application `1535d831578ee600bf6ef2f385c019e1eb569da8`. At the R4 candidate, `src/standalone/save.rs`, `src/git/file_comparison/remote_index.py` and `tests/remote_index_xattr_recording.py` remain byte-identical to 1535. Within the standalone/index/comparison/diff fileviews paths, the only source difference is the intended SVG change. `preservation.json` records exact hashes. Unrelated parent integrations are outside this review.

Cargo.toml conflict resolution retains both rodio 0.21 (default features disabled; playback) and pinned same-file 1.0.6. Compared with preserved fileviews Cargo.toml, the only manifest addition is the parent rodio dependency/comment; the fileviews dependencies remain present, including pinned resvg 0.45.1 (default features disabled; raster-images), roxmltree 0.20.0 and Unix/macOS dependencies. Pure TOML inspection confirms lock entries/checksums for rodio 0.21.1, same-file 1.0.6, resvg 0.45.1 and roxmltree 0.20.0. `dependency-preservation.json` preserves the proof. This establishes merge preservation, not native audio or complete platform behavior.

## FV-R3-1 disposition: resolved

`src/svg.rs:179–188` now refuses local tag name clipPath before creating usvg options and before invoking the parser at `:238`. Local-name matching also catches prefixed clipPath definitions. The pinned usvg converter requires clip-path references to target clipPath elements (R3 preserved dependency `usvg-0.45.1/src/parser/clippath.rs:17–19`). Therefore the actual reported DAG cannot reach recursive mask painting without a refused definition. Attribute, inline style, stylesheet and escaped CSS spelling cannot bypass refusal of the target definition. The check does not rely on CSS reference parsing, resolved output dimensions or unique-definition counts.

The new bounded regression `nested_clip_paths_are_refused_before_parser_for_attributes_and_css` (`svg.rs:270–313`) generates depth 1/4/8 graphs across four reference routes, with two references per next level. Its injected callback verifies no parser entry and an actionable source-view error. Existing marker and source-budget/ordinary-style regressions remain intact. No unrelated production rendering logic changes.

Clipped previews are explicitly unsupported, rather than rendered with omitted clips. `svg.rs:34–45` returns the stored Loading error; refusal does not return NotSupported and silently defer to another renderer. `src/repo/diff/images.rs:89–94` continues to explain unsupported effects and SVG source. `src/repo/diff/view.rs:857–875` preserves the Rendered SVG / SVG source selector and textual path when source is selected. These unchanged paths provide an explicit user-selected fallback; no automatic source switching or newly exercised native GUI behavior is claimed. Benign clipPath definitions are conservatively refused too.

This closes the reported DAG path. It does not introduce hard preemption for an already-running parser/rasterizer, prove a wall-clock guarantee for every accepted SVG feature, or certify all SVG implementation requirements beyond this correction.

## Independent permitted evidence

Compiled a minimal pure/headless harness containing byte-identical candidate render/render_with_parser functions and three exact regression function bodies from immutable source. Existing local egui/resvg/roxmltree/image dependencies were used; paths and hashes are preserved. The harness initializes no application, audio, repository, transport or process service. Compilation and execution both exited 0:

- New clip-path function: all 12 depth/reference-route subcases refuse before parser entry.
- Existing marker function: all 12 depth/reference-route subcases still refuse before parser entry.
- Existing source-budget/ordinary-style function: oversized flat XML refuses before parser entry; ordinary CSS/inline styled SVG actually renders visible pixels.
- Three exact prior R3 repro files, depth 1/4/8, copied unchanged: all now refuse before parser entry with source-view error.

These are three extracted exact regression functions plus three prior bounded cases, not an independently run seven-test libtest group. Evidence: `svg-r4-pure.rs`, `svg-r4-pure`, `independent-{command,result}.json`, `independent-{compile,pure}.log`, `prior-clip-depth-{1,4,8}.svg`. The exact extracted production-function hash is in the command receipt and review manifest. No deep/large/OOM stress input was generated or rendered.

## Supplied parent gates and limits

The parent independently supplied exact 131cd833 seven-SVG-test gate completion: exit 0, **7 passed, 0 failed, 0 ignored**, with source reported clean/exact throughout. I inspected the completed log, copied it as `parent-svg-test-observed.log` and bound its bytes in the manifest. This is supplied parent execution evidence, separate from my pure harness. The log also reports 0 gitcraft_os tests under this filter. The parent all-target locked check was still running when reported; this review does not claim its result. No duplicate cargo test/check was launched by this reviewer.

R3's 19 permitted reviewer passes remain historical evidence on 1535 for unchanged paths; they were not rerun or relabeled as executions on this parent candidate. Windows privileged full-policy and Linux/remote behavior remain unvalidated. No actual Git/helper/SOPS/SSH/mux/credential/process-control/network/native-platform fixture, full suite, CI/PR, tracked source edit, unrelated worktree mutation or subagent occurred. Compiler execution is permitted tooling. Global runtime containment and final combined gates remain parent work.

Machine manifest: `/tmp/gitcraft-51-fileviews-svg-review-r4-manifest.json`. The previous exact1535 R3 FAIL remains valid for its immutable source. This report is scoped PASS for FV-R3-1 and the stated preservation checks only.
