Review workflow ownership
=========================

Each feature module retains its state, lifetime transitions, actions, and tab adapter.
Private child modules own complete operations that can be understood separately:

- `choreography`: source/stage planning, compatibility rehearsal, presentation.
- `contracts`: evidence capture and execution, observation presentation.
- `echo`: personal rulebook persistence/validation, outgoing evidence, presentation.
- `echo_push`: source resolution, separate from captured execution evidence.
- `intents`: local card writes, bounded/validated reads, presentation.
- `radar`: history and test evidence, presentation.
- `reproducer`: sandbox execution, presentation; container ownership stays together.
- `rust_dependencies`: dependency comparison, separate from index construction.
- `storyboard`: anchor reconstruction, durable publication, presentation.
- `continuous`: document membership/row helpers and presentation, separate from tab
  orchestration and the request-owning state.

The existing module facades preserve their API and test module names. Persisted
models remain in their original feature modules, with unchanged serde definitions.
Completion callbacks retain their existing operation settlement and stale display
checks; extracting evidence work must not move those checks in front of mutations
that already started. In particular, a successful write followed by a failed reload
still follows the original feature's completion policy.

The adjacent message modules group exact paint contracts and test fixtures by
producer. Ordinals preserve the original regex precedence. Constant assembly rejects
duplicate ordinals or missing rows, and does not rewrite any raw application data.

This slice starts at `a4d841713af3d31178a3b9212fd3c5adcb4172dc`. Its local verification
uses rustfmt, exact inventory/order comparison, function-body comparison, a scoped
physical-line inventory, and three standalone pure constant-assembly tests. The
lightweight message-module type check uses dependency stubs: it checks module wiring
and constant construction, not localization behavior. Full application builds and
existing Linux/Windows test suites remain integration gates; they were not run in
this implementation worktree. No process-control tests or CI runs were started.
