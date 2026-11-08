# Azure AZ-R1-1 delta review R2 — scoped PASS

Candidate `ada0beb43b608dbf0515fa61b27a22994c9c717b`, tree `04c2e76b780c2443421daeb6c4a8bbb2422f1fcb`, parent `eed8eca6ae8f7cf3e3d82197f5c65c5e77e2e315`. Immutable Git objects only. This closes the single Azure R1 target-equality/revision finding and accounts for the separately disclosed `fb3ecb23590dc8ee8d0b89db448adfd39d1b082c` test-lint cleanup. It is not another whole-provider review or acceptance of inherited Bitbucket work.

**AZ-R1-1 CLOSED.** `src/forge/azure/details.rs:24` adds the native iteration's complete `targetRefCommit` as `iterationTarget` to the serialized revision, while retaining the separate PR `lastMergeTargetCommit`. `:55–62` removes their unsupported equality requirement and instead requires a valid PR target OID. Existing iteration OID validation, nonzero iteration, PR/iteration source synchronization, common commit and native repository/project identity checks remain. This preserves the distinction between source-push iteration history and PR merge-attempt target from the [official iteration API](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-request-iterations/list?view=azure-devops-rest-7.1).

The page's before/after revision comparison still retries drift, and `AzureTarget::guard` still checks captured Context/account, active PR state and exact displayed revision before writes. Azure `mod.rs` and `post.rs` are byte-identical to R1 `6e409553`; the only Azure production change is the five-line details delta. Therefore a fresh historical-iteration page is usable, while both a later PR target change and an independent iteration-target change invalidate its action. Common/source OIDs and positioned-comment mapping are preserved.

Evidence:

- Independent exact extracted `Snapshot`/`snapshot` harness: **3 PASS** in `/tmp/gitcraft-azure-r2-evidence/test.log`. Historical iteration target `b*40` with PR target `d*40` loads and preserves source `a*40`/common `c*40`; PR target and iteration target each change revision independently; invalid PR target OID and mismatched source still refuse. Only inert JSON/repository adapters execute; no application constructor or transport.
- Exact writer recording regression at `azure/tests.rs:271–300` reads the historical-target page and successfully records one fresh comment. A comment from the previous displayed target refuses with zero writes; an independent later iteration-target change refuses, leaving exactly the single authorized write. Existing six Azure cases remain. Supplied exact-candidate log `/tmp/gitcraft-51-azure-r1-fix-ada0beb4-0.log` reports **7 passed**, no ignored tests. This application recording suite was source/log inspected, not independently rerun.
- Supplied exact candidate all-target check PASS (`-1.log`) and **qualified all-target** clippy `-D warnings -A dead_code` PASS (`-2.log`), with commands bound in `/tmp/gitcraft-51-batch-b-validation.json`.

Independent command:

```
cargo +1.95.0 test --offline --manifest-path /tmp/gitcraft-azure-r2-evidence/Cargo.toml -- --test-threads=1
```

The `fb3ecb23` delta was inspected: fixture functions use `Result`, propagated errors and checked predicates instead of unwrap/expect/panic; the recording alias is cfg(test); the existing GitLab Pages implementation is moved intact above tests; one redundant test default update is removed. It does not change provider production behavior. The initial R1 all-target test-lint failure is addressed by the exact later qualified all-target gate; **strict dead-code-clean lint is still a separate, unpassed baseline and is not claimed here**. Native/keyring/HTTP/actual Git/provider behavior is not inferred from lint or compilation.

No actual Git-under-test, az/keyring/helper/CLI, HTTP/provider/account write, native UI, SSH/network, application discovery constructor, process/cancel/timeout fixture, full suite, CI, PR or tracked source edit. Optional native/runtime gates remain held exactly as in R1.

Verdict: **scoped PASS — AZ-R1-1 closed, qualified all-target lint gap closed at this candidate; strict dead_code/native runtime certification excluded.** Previous unrelated feature dispositions remain separate.
