# SSH key helper implementation checkpoint (unvalidated WIP)

Exact queue: issue43 SSH key helper; no overlapping open PR, and PR95 explicitly
leaves this SSH-key case for this helper. Base is reviewed consolidated c56d050.

The first static unit builds inert command plans and recording-oriented assertions.
It is not connected to the app and has not run a process, generated/read a key,
contacted an agent/account/host, or run Cargo. Complete UI/runner wiring and fixture
gates are still required before claiming the feature implemented.

- All steps are explicit. New ed25519 keys live inside an exclusively created new
  user-selected folder; local runner must use create_dir (never create_dir_all),
  remote POSIX script uses mkdir without -p. Existing keys are chosen in place.
- Key generation and agent entry use the existing PTY, with passphrase prompts
  owned by installed OpenSSH. No passphrase/private bytes enter arguments or app
  fields/settings/activity. Upload reads only the selected key's .pub companion.
- Local registration will reuse captured Profile gh/tea account helpers and forge
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
