# Owned remote transport source cut

This branch contains the generic transport extracted from independently statically reviewed Preflight source. It contains no Preflight repair protocol or UI. Runtime and build validation of this standalone cut are blocked by the environment execution hold; the seven preserved pure recording checks passed on the identical generic production code in the source branch.

Use `remote::OwnedDispatch::default()` in the initiating captured Context, then call `remote::exec_owned(host, request, owner, active)` from a worker. The owner is one-shot. Cancellation before the actual send prevents dispatch; cancellation after sending cannot establish that the host stopped. All ambiguous sends, partial writes, flush failures, read loss and deadlines return `OwnedFailure::Unknown`, without retry or the legacy stop selector.

An `Ok(Output)` is a transport completion, not application success. Validate the exit code and your protocol's acknowledgement. On `Unknown`, retain the owner, Context lease, private inputs/recovery and operation barrier until a trustworthy terminal receipt or deliberate recovery. Receipt reads must also use owned transport, with a fresh owner created under the original owner's captured Context. Never infer completion from a missing marker.

The ordinary `remote::exec` path preserves its existing behavior. It is not the mutation API. The new owned local fixture source is not executed under the hold and makes no live SSH/WSL connection.
