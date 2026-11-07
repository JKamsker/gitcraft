# SSH key helper implementation checkpoints

Exact queue: issue43 SSH key helper; no overlapping open PR, and PR95 explicitly
leaves this SSH-key case for this helper. Base is reviewed consolidated c56d050.

The first static unit (1269c54) built inert command plans and recording-oriented
assertions before any execution. Later units below wire the app and add fixture-only
validation. No user keys, accounts or hosts are used during development. Final gates
and independent review remain recorded separately against an immutable commit.

- All steps are explicit. New ed25519 keys live inside an exclusively created new
  user-selected folder; local runner must use create_dir (never create_dir_all),
  remote POSIX script uses mkdir without -p. Existing keys are chosen in place.
- Key generation and agent entry use the existing PTY, with passphrase prompts
  owned by installed OpenSSH. No passphrase/private bytes enter arguments or app
  fields/settings/activity. Upload reads only the selected key's .pub companion.
- Local registration reuses captured Profile gh/tea account helpers and forge
  API. Remote steps execute on that repository machine, using installed gh/tea
  and explicit account metadata there. Local credentials/keys are never copied.
- SSH/WSL use existing Terminal ownership; no new process cleanup, PID selector,
  signals, watchdog, remote sessions, or shared process lifetime controller.
- Native Windows local key folder reservation will use Rust create_dir before
  invoking native OpenSSH in ConPTY. SSH/WSL always use POSIX paths there.
- ssh -T is a direct diagnostic with the chosen key and no additive SSH config or
  agent identities. Explain that forge authentication can succeed with exit1;
  terminal greeting and exit code must be presented rather than guessed success.
- Tea public-key upload requires0.16+ (ssh-keys add --login --title key.pub),
  verified from official Go-module source, not run. Remote alias metadata is
  displayed and confirmed before upload. No new global login is selected.
- Remaining units: captured local registration/API; per-repository setup UI and
  terminal status/lifecycle, Profile-key adoption; inert/fake-tool fixtures,
  appropriate private-target gates and independent review.

The second static unit adds a typed public-key boundary and captured local registration.
The validator strips comments, rejects extra lines/private-key headers, bounds bytes and
checks SSH wire fields. No raw selected file can be passed to local registration. Remote
plans similarly reduce/validate public fields; tea receives a new temporary file containing
only those public fields, never a copy of the original file. Remote GH token lookup/output
is suppressed and its checked named-account token stays transient in shell/environment.
These components remain unwired WIP until the next UI/runner unit; no runtime claim.

The third unit wires the repository palette/window, captured asynchronous workers,
existing PTY status and explicit Profile-key adoption. Generation reserves its new local
folder off the UI thread, and every worker checks the Practice boundary before any action.
The GitHub account check captures an exact command/token environment for both the owner
probe and eventual one-use upload, preventing a later login change from retargeting it.
Only fixture/recording tests are added; no actual key/account action has run. Local compile
and targeted gates are the next step after preserving this unit. CI remains disabled until
the final consolidated work and reviews are complete.

Initial all-target cargo check passed at3c51d18, with an unused import removed in5d2bfaf.
The initial targeted run passed31 tests and exposed one invalid WSL name in a fixture;
1ba6633 corrects that fixture to the accepted distribution-name grammar and adds owned
recording shell/CLI tests. d7f616c adds scoped Forgejo/Gitea loopback request validation,
while remote tea shows actual account API information before explicit upload confirmation.
The opening-UI fixture proves that displaying all four steps creates no key folder,
worker, account request or terminal and does not mutate profile metadata.

Every source unit is committed and pushed before checks/retries. Only installed basic
shell utilities, owned fake SSH/forge tools and owned loopback endpoints are executed by
new runtime tests. No actual ssh-keygen/ssh-add/ssh transport/forge action is executed.
The direct diagnostic intentionally bypasses user aliases/proxies; use the actual server
and port. A missing agent is explained rather than automatically launching a daemon.

The interactive SSH transport sets -a explicitly: a Host ForwardAgent setting must
not load a newly generated remote private key into this computer's forwarded agent.
The agent step requires an agent available on the repository machine. WSL retains
its configured agent (including an explicitly user-configured integration).
