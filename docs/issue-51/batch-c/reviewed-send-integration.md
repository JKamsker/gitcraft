# Reviewed exact-commit send integration

This merge joins batch C `a47c1cd18ae228c59caeee8727bac80476a31b16` and parent `f37ca66b2f1abd24b4af1f2fdd62cb1ba44ed101`. All ten original batch C requirements have source implementations and scoped independent reviews; native/runtime acceptance remains held. Subtree R1 is a static PASS on a47. This adapter delta still needs the parent's finite combined review.

## Consumer contract

Ordinary sidebar/dependency/parent pushes should enter `RepoTab::run_ops_ex` with their original push and expectation steps. That existing route opens the standard preflight modal, checks the selected profile, constructs APPROVED steps, runs reviewer echo, retains original-route/typed-auth receipts, and repeats expected route checks around safety-draft capture. Do not replace it with a direct backend push.

Features with their own durable send receipt, such as outbox/subtree, first prepare a real `push_preflight::Plan` off-thread and display its exact OID, destination endpoints and findings. Explicit sends require the feature's acknowledgement; outbox autosend requires its existing durable plain-push opt-in, current account consent and no findings. Those are caller obligations: the bridge is not a new preflight approval UI.

After those checks, call:

```rust
RepoTab::reviewed_send(&Plan, &egui::Context,
    impl FnOnce(&mut RepoTab, push_preflight::ReviewedSend, &egui::Context)
        + Send + 'static)
```

The bridge runs the existing personal reviewer rulebook flow on APPROVED evidence resolved to the plan's original exact push. Findings/unknown evidence wait for the existing explicit advisory continuation; cancel drops the continuation. At entry the prepared plan must still match the full selected profile/identity policy. Before invoking the continuation, the tab again checks the original repository, full selected profile, immutable connection context and sign-in generation. It retains the actual CommandSource owner rather than resolving an expired opaque marker later. Existing ordinary-operation dispatch remains unchanged.

The continuation starts the feature's normal background mutation worker. Backend publication uses:

```rust
push_preflight::execute_bound_controlled(
    repo, &Plan, &ReviewedSend, credentials, control)
```

`ReviewedSend::check_plan` refuses a different exact push before feature metadata is marked Sending. The worker reuses the unchanged `repo::run_reviewed_push` algorithm: original configured remote RouteBinding is captured before dispatch, the one-shot actual-endpoint receipt must be authorized by that binding, and AuthFailure retains the initiating Context. The same captured binding and retained echo source are rechecked through `execute_controlled_with_guard` before each endpoint write. Existing plan checks, fixed OID/ref/lease, private transport alias, credential policy and rejection/uncertain-outcome handling remain authoritative. `ReviewedSend::from_echo` is internal evidence construction for this UI path (and supplied-state tests), not a public authorization API.

Outbox auto-preparation now finishes off-thread before entering the same echo continuation. It never bypasses an enabled rulebook. Held runtime fixture call sites were adapted to the receipt argument for compilation only; they were not executed.

## Merge ownership

- `src/git/command.rs`: exact parent source, including selected Git, environment clearing/private counted config, credential/practice policy and owned-process dispatch.
- `src/git/mod.rs`, `src/lib.rs`, `src/repo/apply.rs`: additive module/action wiring; retain parent's native helper entrypoint and batch C feature routes.
- `src/repo/operations.rs`: retain parent's APPROVED original-route/auth receipt and repeated EXPECT route proof algorithms; retain batch C plain-push rewrite handling. The shared route/auth helper only gains crate visibility.
- Batch C shared native/index/working/publication transaction sources remain exact a47. Parent file-view publisher sources remain exact parent.
- Parent evidence-archive deletions win. No generated binary evidence is added.

## Evidence and limits

Compilation and audited pure/recording/supplied-state headless checks are recorded outside the repository and bound to the preserved merge commit in the owner handoff JSON. Existing root warnings remain; compilation is not a strict/qualified lint pass. No Git-under-test fixture, helper, subprocess/agent, network, native access/race, PTY, platform runtime, CI or PR gate was run. Parent's first actual native anchor fixture failed; its command-environment fix remains separately owned. Final combined review, containment and authorized native validation remain parent gates.
