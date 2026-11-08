# History merge integration review R3

**SCOPED STATIC/PURE PASS** on `99808ef2726a24801a0042292b95561b05247b5d`, tree `acefcb5c2eb7bdef01e314be36ad7256af147469`. The finite twelve-conflict merge plan, APPROVED authentication bridge, repeated HTTPS route verifier and Gitea refusal-key seam are correctly combined in the inspected source. No blocking merge regression was found in this scope. This is not a new whole-feature review, native authentication/publication validation or final combined-candidate acceptance.

## Immutable binding and work

Parents are history `68ca8a66ed722a19de5864bd3270846ef99a38c5` and accepted root `1a3c6f56435b32a7146ebc31975606ad69994613`. Source was exported with `git archive` to `/tmp/gitcraft-history-integration-r3-99808ef2`; 23 inspected files were byte-compared with immutable `git show <candidate>:<path>` and bound to blobs/SHA-256. Moving writer source was not used for implementation review. Read the full merge plan, prior history Core R2, setup R4, writer merge handoff and exact receipts. The source/gate/input/artifact manifest is `/tmp/gitcraft-51-history-integration-review-r3-manifest.json`.

No tracked edits, source implementation, commits, pushes, CI, PR, subagents, application launch/constructor/discovery, actual Git-under-test, helper/transport/network/native/process-control/filesystem mutation fixture, cancellation fixture or full suite was performed. Source Git object reads and compiler execution are distinct from application runtime fixtures. The later root GitLab/docs changes are outside this immutable merge.

## Twelve conflict dispositions

| Planned conflict | Verified preservation |
| --- | --- |
| `src/forge/mod.rs` | Root selected tea metadata/secret and environment-proxy exports coexist with history module export. |
| `src/git/command/credential_policy.rs` | **Byte-identical to root1a3 and accepted setup8aaa**; blob `5e944a802b6d31266ecfbed817cae0b667356ba6`. HTTPS/version/proactive-auth/late URL policy and retained-directory cleanup were not replaced by old0319 code. |
| `src/git/credential_scope.rs` | **Byte-identical to root1a3 and setup8aaa**; blob `030fa2b21b2eea8c65c7b5762ebf38b9a4364798`. |
| `src/git/command.rs` | One Extra with one owned_pre, one extras application function; root immutable Context/profile/repository/practice construction plus history environment removal and explicit unset are retained. Root helper/module families and history forecast/draft/recovery/reviewed-push helpers coexist. Detailed ordering below. |
| `src/git/mod.rs` | Root bundle-path export plus history draft/push-hooks/PushTarget/repair exports coexist; investigation/recovery declarations are retained. |
| `src/lib.rs` | Root audio, connection doctor and portability coexist with push_preflight. |
| `src/repo/apply.rs` | Additive preflight, dossier/symbol/pinned-line, draft and recovery routes coexist with root DVC, file-comparison and LocalCi routes. Root decrypted/comparison invalidation and history panel transitions remain. |
| `src/repo/dialogs/remote.rs` | Entire file is byte-identical to root1a3, retaining authoritative endpoint choices and HTTPS transient-mode explanations. |
| `src/repo/graph_view/mod.rs` | History take_graph_scroll_row conversion is followed by root graph_motion.take_head_scroll; animated scroll, manual-input and minimap cancellation remain (`:218–244`). Projection rows/selection and ordinary history loading logic are retained. |
| `src/repo/operations.rs` | Preflight interception happens before root motion/celebration launch. Original root auth proof flow coexists with the reviewed bridge; original ref/HEAD expectations and repeated preparation verification remain. Worktree-folder preparation, remote visibility and DVC routing are retained. |
| `src/repo/tests.rs` | Entire file is byte-identical to root1a3, preserving explicitly scoped Credentials::for_url examples. No tests in this file were executed by this reviewer. |
| `src/settings/profiles.rs` | One UserProfile/default includes connections, pending_setup and author_hosts; separate serde-default HostAuthor and longest host/namespace author policy coexist with root Context/ConfigLease profile application. No old global PROFILE_CONFIG setter was restored. |

## Command environment and actual executable: PASS

`command.rs:95–135` removes inherited repository/common-dir/worktree/index/object/alternates/namespace/shallow/replace/config/attribute redirection in git_cmd before intentional profile/per-command values. Local command and piped streaming routes retain the root repository builder (`:233,690,2360`), including the practice repository isolation path. This is not history's older plain git_cmd/with_profile reconstruction.

The single `apply_command_extras:238` applies Extra.unset first; requested credential normalization follows, except for the intentional practice case; pre/owned-pre argv occur once; intentional Extra.env is then installed; the credential alias pins GITCRAFT_GIT_ORIGINAL from the selected Command program; original argv occurs once. Thus normalization does not erase the counted private alias configuration or a deliberate private index. `remote_request:293` carries general/history/credential/extra removals plus command-owned env, and preserves CREDENTIAL_REMOTE_PIN for both traced and ordinary credential transports. Root `remote/session.rs:551` still unsets before exporting request-owned env.

Reviewed push extras (`:1832–1939`) preserve exact URL mapping and cleared-plus-single URL/explicit pushurl for both original remote and private alias, original hooks, LFS settings, receive-pack and physical-object policy. The original-remote push expansion proof precedes actual alias execution. Account/scope validation still precedes secret installation; the accepted shared policy is installed only when scope matches the reviewed endpoint. Ordinary/watched reviewed runners use these same extras (`command.rs:1956`, `command/watched.rs:268`) rather than bypassing the selected builder.

An independent exact-source fixed-Command adapter passed assertions for removals→normalization→owned env, retained selected profile/SSH choice, GIT_CONFIG_COUNT/private configuration, private index, physical objects, selected absolute executable, original lease/refspec argv, single credential alias, both remote tracing modes, and the practice normalization exception. Commands used fixed `/recorded/*` executables and were only inspected. Generated shell strings were never executed.

## APPROVED authentication bridge: PASS

`operations.rs:913–926` obtains `push_args(&plan)` before the transport substitutes its private alias. `run_reviewed_push:643` captures root RouteBinding from those original-remote arguments before execution and captures the initiating immutable Context. It refuses dispatch if route capture fails. It does not capture the NUL APPROVED marker or use an opaque alias as retry authority.

`reviewed_auth_failure:671` consumes `take_auth_endpoint(error)` once, requires authoritative `binding.authorizes_endpoint(endpoint)`, then supplies an AuthFailure with the initiating Context, that exact endpoint and captured binding to OpControl. The returned/logged error remains opaque. Missing/expired or foreign receipts produce no authorization. `op_done:466` explicitly refuses reviewed recovery without this typed proof; there is no default-fetch URL fallback.

The root authoritative RouteBinding and HTTPS recovery modules are byte-identical to root1a3. Their accepted ticket endpoint authority, Context/generation/path/remotes currency, secondary endpoint partition, pre/post rejection verification and retained retry proof remain. Ordinary run_step failure capture remains the root path. Sign in still serializes EXPECT_HTTPS_ROUTE into the retained operation before retry (`operations.rs:340–360`).

Independent exact-source inert checks passed original-argument capture-before-dispatch, capture failure without execution, accepted exact one-shot endpoint/context/binding, consumed/expired receipt refusal, and fetch/other-path/LFS/HTTP foreign endpoint refusal. Route membership uses the actual accepted CredentialScope and RouteBinding authorization function. Metadata capture, real credentials/rejection/transport and the full application UI were not executed by the adapter.

## Repeated verifier and retry: PASS

`run_steps_counted:845–876` includes `check_https_routes` inside the same verifier closure as original expected refs and approved/undo HEAD, passed through production verify→draft capture→verify. A successful safety draft therefore cannot move the last HTTPS route proof ahead of that potentially long capture. EXPECT markers are skipped during command dispatch, remain recognized as expectations, and are retained by keep_unfinished (`:174–188,879,1088`).

The independent adapter ran the actual check_https_routes and with_rechecked_preparation functions with an inert verifier. A route changed during successful preparation produces exactly two checks and refusal before dispatch. Malformed serialized proof refuses before the verifier is called. This proves the glue ordering with supplied state; it does not claim atomic exclusion of external configuration writers between final verification and Git's own operation.

## Gitea refusal key and unchanged ownership boundaries: PASS

`forge/gitea/mod.rs:794` adapts history_api to root `.on_refused_key(forget_login, &login_key(&hosting.web))`, matching the ordinary selected-account path. Api stores an owned copy of that key (`forge/http.rs:94`), and the bounded history HTTP401 path invokes it (`:140–145`). The callback forgets the captured cache entry, rather than recomputing a key using a later profile. This resolves the removed on_refused compile seam without altering transport or account-selection policy. No HTTP/keyring/forge fixture was run.

History push_preflight, rebase/rewrite guards and push-fix transaction are byte-identical to history68ca. Root endpoint/recovery/dialog/test files and file-comparison index/worktree transactions are byte-identical to root1a3. The merge does not introduce or modify BatchC working_transaction/native_files work. Their separate review/ownership remains separate; no new native transaction certification is implied.

Prior history Core R2's shared0319 dependency FAIL is closed here by retaining the accepted setup8aaa source, as scoped by setup R4. That removes the named old netrc/cleanup source objections; native credential/platform acceptance remains held. Previously accepted history content/core source behavior is preserved except for the explicitly reviewed scroll/motion and command/auth glue above. Original feature coverage is not re-reviewed or expanded.

## Gates, provenance and limitations

Independent inert adapter and seam inventory are under `/tmp/gitcraft-history-integration-r3-evidence`; generator `/tmp/gitcraft-history-integration-r3-adapter-build.py`. **Five distinct tests passed, zero failed.** It embeds exact immutable production policy/scope modules before cfg(test), builder policy functions, route authorization, receipt store and bridge/preparation helpers. Context, practice membership, remote types/quoting, diagnostic scrub and OpControl storage are inert substitutions. No metadata acquisition, git_cmd/local_command/repository execution, task pool or app constructor is embedded.

Compilation used Rust1.95 and existing serde artifacts; this is not a rebuild of gitcraft. One initial adapter-only compilation failed because the harness lacked the surrounding hooks alias/TOOL_FOR constant and selected an incompatible serde_json artifact. The adapter wiring and dependency selection were corrected; the second compilation passed and its five tests ran once. Production source was unchanged. Generated scripts remained data.

Independently verified all **26 supplied writer receipts**, exact candidate SHA, exit0 and each log's SHA-256, with **32 distinct selected test names** across21 filters plus five build/check/lint/fmt gates. This is inspected writer execution, not reviewer reruns of application binaries or compilation. Qualified library lint explicitly permits existing dead_code. The old 68ca set remains25 strict plus1 discovery-capable, and earlier02d sets18 plus1; no old gate was retroactively relabeled. Current shared credential policy preserves the accepted fixed-Command test.

Held: actual Git/helper/credential-fill/reject, physical push/ref-routing/lease/upstream effects, HTTP/SSH/PTY/network, native platform/filesystem/process/cancellation/cleanup and GUI fixtures, full suite, CI/PR and exact-final combined acceptance. Nothing in this merge review lifts that hold or reopens accepted unchanged policy/features. No additional source correction is requested for the finite integration scope.
