# Jira/Linear two-finding correction review R2

**PASS for TR-R1-1 and TR-R1-2 on exact `f9f4da0c179ef65f9bd6743c321535301d37db32`. Both findings are closed in the inspected source and inert evidence scope.** Provider, secure-store, Git and native acceptance remain held; this verdict does not release those gates.

## Binding and scope

Candidate tree: `09b36055572814c3482c1bebef170fd54ae93376`. TR1 correction `b5b7387205447a5fcfd98d50e49a49a524665c22` is an ancestor of the candidate; TR2 is the candidate commit. Reviewed immutable Git objects and copied relevant files byte-for-byte into `/tmp/gitcraft-trackers-r2-f9f4-source`, without reading moving writer source. Companion manifest binds the files, exact extracted functions, adapters, dependencies and inspected receipts. Read the full original R1 report and manifest and the original tracker handoff. Scope is limited to their two functional findings and directly changed criteria plumbing. Previously accepted GitLab/shared credential boundaries are not reopened.

## Finding dispositions

| Finding | Disposition | Source-bound evidence |
| --- | --- | --- |
| TR-R1-1: duplicate tracker-row widget IDs | **Closed.** | `src/forge/mod.rs:216–221` identifies tracker issues by kind/origin/stable ID, independent of numeric zero, display key or title. Forge identity retains URL/number. `src/repo/issue_work/picker.rs:424` uses that identity for actual egui interaction. |
| TR-R1-2: configured Jira criteria lost behind an earlier description heading | **Closed.** | `src/trackers/mod.rs:312–324,374` preserves the description and stores the explicitly configured field separately as `Issue.acceptance`. `src/repo/issue_work/link.rs:46–59` takes that override before description extraction and applies the existing card cap. |

TR1's independent headless adapter extracts the exact candidate `issue_row` and `Issue::identity` methods. It supplies two inert tracker issues and actual egui pointer press/release input, preserving the original failing geometry. Results now are:

```text
row0 [8,8]–[592,36]; row1 [8,39]–[592,67]
click row0 -> [true, false]
click row1 -> [false, true]
```

This directly reverses both original failures. The source-qualified identity also remains stable when a tracker key/title changes. The writer's actual `choose` headless regression at `picker.rs:542–597` exercises repeated pointer selections and ArrowUp/ArrowDown after filter focus, asserting selected issue and suggested branch. Its exact-candidate receipt passes; it was inspected rather than independently rerun. Independent execution here covers the actual row interaction, not the entire Picker or OS keyboard integration.

TR2's independent pure adapter extracts the exact candidate `Client::parse`, `required`, `adf` and `IssueLink::of`, plus criteria extraction, line conversion and cap helpers. Supplied JSON reproduces the original conflicting description/custom-field case. The description remains `## Acceptance criteria\nold description requirement`, while the kept card data becomes `## New requirements\n\n- [ ] selected custom-field requirement`; card-line conversion yields the selected requirement as a task. Neither duplicate-heading extraction nor replacement of the original description is involved.

Additional bounded assertions passed: unset field retains existing description extraction; configured null becomes explicitly empty `Kept::Criteria`; omitted configured field returns the explicit field/permissions error; long Unicode content is capped on character boundaries at 4000 characters plus newline/ellipsis, and 100 supplied lines become 60 lines plus ellipsis. `criteria.rs:291–315` supplies these existing caps. `link.rs:105` writes the resulting criteria into the existing branch-link config path; `card.rs:117–132` reads those exact kept lines and discloses an empty selected field as “No acceptance criteria are recorded for this issue.” Settings `trackers/ui.rs:24` explains the authoritative-field policy, including emptiness. Writer's real parse→IssueLink→card-line regression corroborates the same composition. Refresh already shares parse/link conversion, so it receives the same corrected behavior.

The new `acceptance` field defaults to None in the changed GitHub/GitLab/Gitea issue constructors, and Linear parsing keeps None; those paths retain existing description extraction. No adjacent provider behavior was recertified.

## Gates and execution classification

Inspected exact-f9 writer receipt and all five logs, not rerun: seven tracker pure/recording tests, six picker pure/headless tests, five link pure tests (**18 passes; no ignored tests**), offline all-target check and qualified library Clippy `-D warnings -A dead_code`, all exit 0. The qualification is retained; this is not a claim of unqualified strict lint. Receipt selection excludes unrelated batch entries and historical actual Git fixtures.

Independent reviewer execution: **two rustc +1.95.0 compilations and two adapter runs, all exit 0**. One run is inert egui headless pointer input; one is pure supplied-JSON parsing/link/card-data assertions. Row styling/chip and data types are inert shims; the identity kind uses a supplied serializable string rather than the production enum. Criteria types are inert equivalents, `today()` returns a recorded string, and the unrelated Markdown `plain` renderer is omitted. Exact reviewed functions remain byte-identical to their Git-object spans, with hashes in the manifest. No App/RepoTab/client/discovery constructor, keyring/store operation, actual Git/helper/CLI/process fixture, HTTP/network/provider query, native window/keyboard/screenreader, signal/cancel/timeout fixture, full suite, CI, PR, tracked edit or subagent occurred.

This closes the two demonstrated original R1 source defects. Existing future provider/store/Git/platform acceptance stays separate under the unchanged runtime hold.
