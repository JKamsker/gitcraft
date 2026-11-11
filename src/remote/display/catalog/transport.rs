//! Exact diagnostic bindings owned by transport behavior.
pub(super) const MESSAGES: &[(&str, &str, Option<u32>)] = &[
    (
        "no app to ask",
        "interaction-remote-no-app-to-ask-786f42",
        None,
    ),
    (
        "oversized protocol frame",
        "interaction-remote-oversized-protocol-frame-88ad94",
        None,
    ),
    (
        "incomplete protocol frame",
        "interaction-remote-incomplete-protocol-frame-167d04",
        None,
    ),
    (
        "protocol write stopped",
        "interaction-remote-protocol-write-stopped-0153bd",
        None,
    ),
    (
        "absolute protocol deadline expired",
        "interaction-remote-absolute-protocol-deadline-expired-89de09",
        None,
    ),
    ("not a file", "interaction-remote-not-a-file-2cd310", None),
    (
        "it is a symbolic link",
        "interaction-remote-it-is-a-symbolic-link-f4fd09",
        None,
    ),
    (
        "can't copy a file between two machines",
        "interaction-remote-can-t-copy-a-file-between-two-machines-a1542e",
        None,
    ),
    (
        "remote shell unavailable",
        "interaction-remote-remote-shell-unavailable-a6840b",
        None,
    ),
    (
        "missing output completion endpoint",
        "interaction-remote-missing-output-completion-endpoint-08c523",
        None,
    ),
    (
        "no endpoint to retire",
        "interaction-remote-no-endpoint-to-retire-43ee9a",
        None,
    ),
    (
        "no ownership handshake; cleanup remains unknown",
        "interaction-remote-no-ownership-handshake-cleanup-remains-unknown-01597e",
        None,
    ),
    (
        "the supervisor did not confirm its atomic stop/reap receipt",
        "interaction-remote-the-supervisor-did-not-confirm-its-atomic-stop-reap-receipt-cbce6c",
        None,
    ),
    (
        "private endpoint evidence retained",
        "interaction-remote-private-endpoint-evidence-retained-b5d669",
        None,
    ),
    (
        "owned command stopped but output delivery is incomplete or unconfirmed",
        "interaction-remote-owned-command-stopped-but-output-delivery-is-incomplete-or-unconfirmed-b6a082",
        None,
    ),
    (
        "it took too long; the owned supervisor confirmed stop and reap",
        "interaction-remote-it-took-too-long-the-owned-supervisor-confirmed-stop-and-reap-f892b4",
        None,
    ),
    (
        "it took too long; cleanup is unknown and recovery evidence was retained",
        "interaction-remote-it-took-too-long-cleanup-is-unknown-and-recovery-evidence-was-retained-a03af7",
        None,
    ),
    (
        "remote controller closed",
        "interaction-remote-remote-controller-closed-936232",
        None,
    ),
    (
        "the connection closed",
        "interaction-remote-the-connection-closed-fb422b",
        None,
    ),
    (
        "it didn't answer in time",
        "interaction-remote-it-didn-t-answer-in-time-7ec0bb",
        None,
    ),
    (
        "it answered out of turn; cleanup unknown",
        "interaction-remote-it-answered-out-of-turn-cleanup-unknown-0a6d04",
        None,
    ),
    (
        "invalid ownership handshake; cleanup unknown",
        "interaction-remote-invalid-ownership-handshake-cleanup-unknown-fd5e10",
        None,
    ),
    (
        "missing terminal receipt; cleanup unknown",
        "interaction-remote-missing-terminal-receipt-cleanup-unknown-43cde7",
        None,
    ),
    (
        "invalid output header",
        "interaction-remote-invalid-output-header-b2210f",
        None,
    ),
    (
        "owned command stopped but output delivery is incomplete",
        "interaction-remote-owned-command-stopped-but-output-delivery-is-incomplete-8da45f",
        None,
    ),
    (
        "controller lost; cleanup unknown",
        "interaction-remote-controller-lost-cleanup-unknown-e1e737",
        None,
    ),
    (
        "output delivery lost",
        "interaction-remote-output-delivery-lost-d05d87",
        None,
    ),
    (
        "oversized ownership record",
        "interaction-remote-oversized-ownership-record-42ba81",
        None,
    ),
    (
        "invalid ownership endpoint",
        "interaction-remote-invalid-ownership-endpoint-79c82e",
        None,
    ),
    (
        "invalid request nonce",
        "interaction-remote-invalid-request-nonce-777b5d",
        None,
    ),
    (
        "remote input exceeds 64 MiB",
        "interaction-remote-remote-input-exceeds-64-mib-de2fb3",
        Some(64),
    ),
    (
        "invalid remote execution deadline",
        "interaction-remote-invalid-remote-execution-deadline-94cb62",
        None,
    ),
    (
        "invalid standalone capture limit",
        "interaction-remote-invalid-standalone-capture-limit-879799",
        None,
    ),
    (
        "SSH multiplexing is unavailable",
        "interaction-remote-ssh-multiplexing-is-unavailable-9c3e39",
        None,
    ),
    (
        "dynamic SSH Match condition",
        "interaction-remote-dynamic-ssh-match-condition-8b6012",
        None,
    ),
    (
        "relative system SSH Include",
        "interaction-remote-relative-system-ssh-include-b64630",
        None,
    ),
    (
        "SSH config root needs literal filename handling",
        "interaction-remote-ssh-config-root-needs-literal-filename-handling-0cd61a",
        None,
    ),
    (
        "SSH configuration path cannot be quoted",
        "interaction-remote-ssh-configuration-path-cannot-be-quoted-542feb",
        None,
    ),
    (
        "this portable copy of gitcraft doesn't use the system's credential store (Settings → General → Portable mode)",
        "interaction-remote-this-portable-copy-of-gitcraft-doesn-t-use-the-system-s-credential-store-set-10836a",
        None,
    ),
    (
        "no folder for this app's files",
        "interaction-remote-no-folder-for-this-app-s-files-15138c",
        None,
    ),
    (
        "gitcraft: invalid ownership handshake",
        "interaction-remote-gitcraft-invalid-ownership-handshake-2aa41f",
        None,
    ),
    (
        "gitcraft: oversized ownership handshake",
        "interaction-remote-gitcraft-oversized-ownership-handshake-f49fa3",
        None,
    ),
    (
        "gitcraft: safe execution needs Python 3 POSIX support or Perl with its core POSIX/JSON modules; install either on this machine",
        "interaction-remote-gitcraft-safe-execution-needs-python-3-posix-support-or-perl-with-its-core-p-89a11a",
        None,
    ),
    ("cancelled", "interaction-remote-cancelled-8b4704", None),
];
