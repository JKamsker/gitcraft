# Behaviour timeline (#34.9)

Source implementation after `a67afada3b7d7c167c8a3bffae2699b16ddb7050`;
independent review and runtime gates pending. No supervisor architecture changes.

**Behaviour timeline…** in the command palette opens a real dialog. Load frames
resolves an ending revision to a full commit ID and reads at most 32 first-parent
commits, oldest first, with pinned tree identities and bounded metadata. Passive
reads reuse the existing no-lazy-fetch policy. Changing the selected revision/count
clears the old frame list. A separate **Run command twice per missing frame** button
confirms the typed command, mode, machine and timeout before any worktree mutation.
Commands can run hooks/filters or access the network with the user's account.
Repository writes queue behind the batch's existing mutation key.

The runner reuses the existing bisect Workspace's actual-machine generation claim,
mutation fences, durable dispatch owner records and checked cleanup. Each repeat
gets a fresh detached worktree; no build products carry between repeats. Local,
WSL and SSH repositories use the same established managed execution path. A result
is publishable only after both exact observations and workspace cleanup succeed.
Concrete setup/owner/worker/cleanup errors are preserved before cancellation checks;
unknown cleanup invalidates the workspace and retains recovery evidence. Stop uses
the ambient owned job token; queued cancellation has an explicit completion handler.
Closing cancels the child scope, while shared Jobs/Service retain their join duties.

Modes are complete UTF-8 stdout, one finite benchmark scalar, or a rendered PNG
written to stdout. PNGs are decoded to RGBA for pixel comparison, with 1024×1024
strict dimension limits and a 16 MiB decoder allocation budget. Exit code and exact
stderr bytes also participate in repeat equality; nonzero normal exits can be stable
observations when their stdout satisfies the mode. Invalid/signal/timeout/oversized
outputs are failed/unavailable frames. Accepted stdout/stderr are each at most 2 MiB;
the existing transport separately captures at most 64 MiB per stream before these
application checks. These are bounded complete observations, never successful
truncation. Error stderr has an explicitly bounded preview.

The slider and clickable frame list scrub observed text, numbers and screenshots.
Stable numeric points are clickable and never interpolate gaps. **Open first observed
adjacent change** navigates to the corresponding commit. This means the first unequal
adjacent stable pair in the loaded range, not a guessed global culprit across missing
or inconsistent samples. Unrun/evicted, queued/running, failed and inconsistent repeats
are labelled separately. The tab-lifetime cache is keyed by repository/machine path,
tree, trimmed exact command and mode; it retains at most 64 keys / 32 MiB of output
payload. Loaded frame values and the active image texture have additional bounded
storage (at most 32 frames, each at most 4 MiB RGBA plus 2 MiB stderr). Machine/tool
configuration, commit metadata and working directory paths can still affect commands;
the UI explains the tree-cache assumption and provides Forget cache / rerun.

Audited pure gates only: metadata parsing; repeat equality including exit/stderr;
unknown/flaky gaps and adjacent change selection; copy/tree/command/mode cache
separation and eviction; UTF-8/scalar/output bounds; in-memory PNG pixel decoding and
oversized dimensions; primary+cleanup errors; injected egui frame rendering and late
ownership errors after a cancelled token. None opens Git, starts helpers/children,
connects to remote machines or creates worktrees. Runtime local/WSL/SSH/Windows,
actual command/checkout/crash recovery and GUI interaction fixtures remain held.
