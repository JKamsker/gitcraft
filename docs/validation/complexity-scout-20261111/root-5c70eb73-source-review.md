# Independent root-scope source review

Reviewer: gate_scout. Reviewed immutable candidate:
`5c70eb73b12002f9c84c8e06d96f7c229e433d6a`.
Comparison base: `2b370d4eb7c000748bb81c3dafaa8bd23bc596b5` (`dev` at review).
No application builds or runtime tests were run for this review. This is scoped
source-review evidence, not approval of a later combined candidate. The scout's
own Git/Insights/parser and policy implementations are excluded.

No blocking findings were identified in the reviewed root scopes:

- Process `execution/completion.rs`: the final let-chain still invokes completion
  only when no prior stop exists and the callback is present; only callback failure
  records a stop and prevents retirement. The earlier `69d23585` capture/completion
  source review compared ownership, cancellation, drain ordering, and callback
  failures against the prior implementation; this final delta preserves those.
- Insights `graph.rs` and `graph/scene.rs`: the radius expression, edge/node paint
  order, focus/dimming, clipping-response indexing, labels, and tooltips are retained.
- CI waterfall and `waterfall/dependencies.rs`: rows retain job/expanded-step order;
  dependency selection, timestamps, evidence styles, and arrow geometry are retained.
- Root fixture extraction in remote native-owner/workspace-claim tests, transaction
  outcomes, resolver flow, remerge, parallel-bisect warning presentation, Windows
  metadata, and Fluent agent/doctor/workflow/review/control-label assertions retains
  their assertion values, call order, and owned fixture lifetimes. The resolver's
  reused `conflicted` helper has the same setup commands as the removed inline code.

An initial concern about transaction `diagnose_capability` scanning a global temp
directory was withdrawn after tracing its only call path. `owned_child` exclusively
creates a case root, sets mode 0700, creates its `tmp` child, clears inherited
environment, sets `TMPDIR` to that child, and invokes the exact test/case in the
child. Default suite cases do not request the diagnostic case. Thus this scan is
inside an owned namespace; a duplicate speculative refactor is unnecessary.

Source is ready for the separately required runtime sequence: successful synthetic
and recording checks first, a fresh PID/proc/device containment probe, then owned
child suites with clean environment and private temporary/config/data/cache paths.
`bwrap --unshare-pid --die-with-parent --bind / / --dev /dev --proc /proc` contains
process visibility; the runner must retain its owned child handle and use no
host-wide process selection or signals. A PID namespace alone does not isolate
filesystem temporary files; the private directory/environment setup is required.
