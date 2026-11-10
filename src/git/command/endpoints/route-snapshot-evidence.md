# Route snapshot envelope follow-up (#198, #193)

The prerequisite `3fd9f5eebee599e75e30ee58a83d2a5abc4e2aa5` was applied
locally as `4726fe828483608a51cc7f2214c056cf161e5c96`. Its committed
`src/git/**` tree matches the original exactly. This follow-up is a separate
commit; it does not modify `src/repo/**` or adopt the parser there.

## Interface and policy

Public signatures, serialized RouteBinding fields, and original execution
arguments stay unchanged. A private RouteQueries policy uses the shared syntax
parser and retains the exact global prefix for all endpoint inventory,
branch/config selection, literal rewrite, and snapshot config reads. The
selected-repository remote_endpoints API still reads its selected repository.

Supported route-query context: attached/separate -C, -c, --git-dir,
--work-tree, --namespace; --bare; --no-pager/-P; --no-replace-objects;
--no-lazy-fetch; --no-optional-locks; and the four existing pathspec flags.
Repeated options and their order are retained. Values are not reinterpreted as
commands, even when named fetch/push. Git remains responsible for validating
config assignments and path values.

Conservative errors before metadata reads: unknown/incomplete envelopes,
internal markers in the envelope, --config-env (ambient values are not bound
across normalized execution), --paginate/-p, and --super-prefix. The latter
options are not supported by this proof policy even when syntactically known.
These new errors are static English strings; existing missing-implicit-remote
and snapshot-mismatch errors remain. Empty input retains its prior empty-route
behavior. Subcommand option parsing and implicit remote selection rules were
not expanded by this change.

## Local evidence

`python3 src/git/command/endpoints/recording_harness.py`:

- 6 recording-only route tests pass, using actual endpoint bodies and a private
  injected reader. A and B both have origin, with distinguishable URLs.
- 4 shared-parser tests and 4 pure literal-rewrite tests pass.
- A temporary mutation dropping query context fails the recording assertions.
- std-only rustc --test type-check passes. The harness removes only serde derives
  from a temporary copy and stubs profile, translation, and credential-scope
  services. The production process boundary panics if reached.

Fixtures cover attached/separate repository options, repeated -C, branch
selection, fetch --all, explicit/implicit push/fetch/pull, -c override and literal
rewrite queries, values named fetch/push, unsupported envelopes with zero reads,
unchanged A versus changed B, and config-only snapshot changes.

`rustfmt --edition 2024 --check src/git/command/endpoints.rs` and
`git diff --check` pass. Full-tree working/index file-size gates report 163
inherited errors outside src/git, matching the prerequisite revision. Every
tracked Rust file under src/git is at most 500 physical lines. The full-tree
size gate is not claimed to pass.

## Limits

No application Cargo build, actual Git operation, process/network test, CI,
push, PR, or independent review was run. The recording model verifies query
construction and snapshot comparison, not Git's runtime semantics, serde,
platform behavior, profile handling, or credential matching. Invalid config
values named fetch/push are syntax-preservation fixtures, not evidence Git
accepts those assignments. Existing application gates must be run by the parent
on its final combined candidate. This evidence belongs to the commit containing
this file; it does not claim application-wide routing correctness.
