Repository-tools ownership and re-review fix for #198

Provenance: inherited HEAD 0498aeee has a repository_tools tree identical to
77d8e0c8d728ada555f5d8820b79d70cddfca1cb. The owner follow-up
38db38560db33b8eb5d6048be8a36668175ee50b was missing. Its complete feature-scope
implementation is restored here, then extended to cover the re-review findings.
No files outside src/repo/repository_tools were changed by this unit.

Owner constructs state, issues request identities, resets preparation consent,
and accepts or discards completion success/errors. All specialized preparation
callbacks (virtual branches, inverse previews, subtree/outbox, isolated replay,
publication, dependencies and automatic outbox) now use that policy. New requests
advance identity; alpha's old success/error cannot replace beta's newer review.
Existing action API, persisted data, command execution and test names remain.

Central integration contract (required before application delivery):

    pub(in crate::repo) fn invalidate_repository_tool_requests(&mut self)

Invoke synchronously on EVERY repository/profile/connection-context invalidation
and sign-in generation change, and BEFORE suspend/channel receiver replacement,
channel disconnection or reconnect/resume establishes a new request lifetime.
Call on each transition, even if a repository/profile/context returns to its old
value before the next frame or callback. The adapter takes no egui context and
is safe to call repeatedly. It advances a checked monotonic per-tab epoch, clears
pending deferred/automatic-outbox/poll identities, resets deferred reading and
poll times/cache markers, resets outbox reading/timing, clears consent/results
and closes only a RepositoryTools dialog. Other dialog kinds are preserved.

It deliberately preserves executing-operation handles, busy/accounting state,
undo/persistence and mutation settlement. Central lifecycle must preserve/drain
executing mutation completion delivery across receiver replacement so op_done
still runs exactly once; this adapter neither cancels a mutation nor settles it.
Mutation/publication/dependency callbacks call op_done before stale UI discard.
Do not replace the deferred owner with Default during resume (that resets epoch).

Feature adapters additionally observe repository/config path, tab and selected
connection Context, profile id/connection choices and HTTPS sign-in generation
before starts/polls/completions, advancing epoch when locally observed values
change. Tickets contain that epoch and context. Snapshot observation alone cannot
see an A→B→A between calls: central transition hooks above are mandatory. Central
files belong to another active agent and are intentionally untouched here.

Pure/source validation (no full Cargo/application/process tests):

- python3 src/repo/repository_tools/pure_checks.py: 15 owner/cache/lifecycle/adapter
  tests plus 3 effect-routing tests pass. The harness compiles actual production
  owner, lifecycle, context adapter and cache declarations/implementation; only
  external payloads/settings/tab plumbing are inert. It covers A→B→A old success
  and error, same-id detached owner epoch rejection, alpha/beta out-of-order,
  close/reopen, suspend/resume, connection/sign-in transitions and cache reset.
- Source assertions check production mutation op_done ordering, invalidation's
  preservation of executing accounting and owner-only preparation publication.
  They do not claim execution of real mutation scheduling or channel replacement.
- rustfmt recursive check and git diff whitespace check pass.
- Complete feature tree is below the 500-line hard cap. Required complete-tree
  working/index size checks still fail on out-of-scope files (49 at this snapshot).
- Existing tests remain in their original modules, including inherited owner tests.

Status: locally pure/source validated. Parent must integrate the central hooks,
compile and test the full application, retain independent review evidence on the
final candidate, and follow final-candidate CI policy. No push, PR, CI, process
control test or subagent was used for this unit, as explicitly requested.
