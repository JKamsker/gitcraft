backend-inbox-no-login = No { $forge } login is known for { $host }: add one with `tea login add`, or open a repository with a login so gitcraft can identify your pull requests.
backend-noun-merge-request = merge request
backend-noun-pull-request = pull request
backend-just-now-7ddb44 = just now
backend-relative-days = { $count ->
        [one] { $count } day ago
       *[other] { $count } days ago
    }
backend-relative-hours = { $count ->
        [one] { $count } hour ago
       *[other] { $count } hours ago
    }
backend-relative-minutes = { $count ->
        [one] { $count } minute ago
       *[other] { $count } minutes ago
    }
backend-relative-months = { $count ->
        [one] { $count } month ago
       *[other] { $count } months ago
    }
backend-relative-weeks = { $count ->
        [one] { $count } week ago
       *[other] { $count } weeks ago
    }
backend-relative-years = { $count ->
        [one] { $count } year ago
       *[other] { $count } years ago
    }
backend-search-date-formats = 2024-05-01, 2024-05, 2024, today, yesterday, or a time ago: 12h, 3d, 2w, 6m (months), 1y
backend-value-is-not-a-date-write-date-formats-at-most-max-years-years-ba-cd7a9d = “{ $value }” is not a date: write { $date_formats } (at most { $max_years } years back, days up to { $last_year })
backend-account-action-cancelled-13efa7 = Account action cancelled.
backend-account-check-cancelled-34de37 = Account check cancelled.
backend-analysis-cancelled-d5fe01 = Analysis cancelled
backend-atlas-cancelled-b9165d = Atlas cancelled
backend-bounded-forge-history-lookup-failed-or-was-cancelled-41e4a7 = Bounded forge history lookup failed or was cancelled
backend-cancelled-before-this-destination-e79896 = Cancelled before this destination
backend-comparison-cancelled-fe0a3c = Comparison cancelled
backend-content-recovery-cancelled-or-timed-out-b878c7 = Content recovery cancelled or timed out
backend-draft-cancelled-426ba2 = Draft cancelled
backend-draft-permissions-cancelled-52db5c = Draft permissions cancelled
backend-export-cancelled-dbef83 = Export cancelled
backend-file-preview-cancelled-1db629 = File preview cancelled
backend-file-recovery-cancelled-or-exceeds-2-mib-2d111b = File recovery cancelled or exceeds 2 MiB
backend-forge-history-response-exceeded-its-budget-or-was-cancelled-233e64 = Forge history response exceeded its budget or was cancelled
backend-frame-cancelled-4d6f8f = Frame cancelled
backend-gitlab-operation-cancelled-d11bad = GitLab operation cancelled
backend-history-evidence-request-budget-reached-or-cancelled-8d8d5d = History evidence request budget reached or cancelled
backend-history-lookup-cancelled-9533a8 = History lookup cancelled
backend-ignore-edit-cancelled-before-opening-012613 = Ignore edit cancelled before opening
backend-ignore-edit-cancelled-before-publication-80ba37 = Ignore edit cancelled before publication
backend-ignore-preparation-cancelled-a562fd = Ignore preparation cancelled
backend-ignore-preview-cancelled-d6374a = Ignore preview cancelled
backend-line-history-cancelled-or-exceeded120seconds-7448a3 = Line history cancelled or exceeded120seconds
backend-not-run-cancelled-02b0ef = Not run: cancelled
backend-object-inspection-cancelled-e67f16 = Object inspection cancelled
backend-provider-request-cancelled-54eb9a = Provider request cancelled
backend-provider-request-cancelled-refresh-to-establish-any-write-outcome-70b94f = Provider request cancelled; refresh to establish any write outcome
backend-public-key-upload-cancelled-before-sending-eecb8c = Public-key upload cancelled before sending.
backend-push-rejected-cancelled-or-outcome-uncertain-inspect-the-destinat-78d1f3 = Push rejected, cancelled or outcome uncertain. Inspect the destination before queueing again.
backend-read-cancelled-045da9 = Read cancelled
backend-recovery-cancelled-before-mutation-922fff = Recovery cancelled before mutation
backend-recovery-cancelled-d0dd9f = Recovery cancelled
backend-replay-cancelled-c6ccd5 = Replay cancelled
backend-review-cancelled-519921 = Review cancelled
backend-saved-graph-view-cancelled-754f0e = Saved graph view cancelled
backend-setup-operation-cancelled-cdccbd = Setup operation cancelled
backend-symbol-history-cancelled-or-reached-its-time-budget-6f2781 = Symbol history cancelled or reached its time budget
backend-symbol-parsing-cancelled-9fccaf = Symbol parsing cancelled
backend-symbol-parsing-cancelled-or-exceeded-structural-budget-b3ef57 = Symbol parsing cancelled or exceeded structural budget
backend-symbol-parsing-cancelled-or-exceeded-two-seconds-901d02 = Symbol parsing cancelled or exceeded two seconds
backend-working-folder-publication-cancelled-a8f541 = Working-folder publication cancelled

backend-contained-bundle-error = Contained bundle Git: { $detail }

backend-helper-display-06729de987 = invalid keep ownership

backend-helper-display-124d998466 = Safe index reads require Perl with Fcntl no-follow support on the repository machine

backend-helper-display-18d2a159d4 = Safe remote staging requires Python 3 standard-library no-follow file and ACL APIs; install python3 on the repository machine

backend-helper-display-219d77f375 = DVC is not installed on this repository machine

backend-helper-display-286c0e8cfe = Bundle unpacking requires Linux hard address-space limits on this host; no pack was opened.

backend-helper-display-3135c24fb2 = Unsupported MD5 capability

backend-helper-display-389db1e9f2 = invalid resource helper arguments

backend-helper-display-46b86a42f2 = invalid keep message

backend-helper-display-4b3bf71f7e = keep identity changed; retained

backend-helper-display-50ca4f5bd0 = invalid keep retirement arguments

backend-helper-display-7f609a6911 = Git did not return an exit code

backend-helper-display-8322e87d24 = stopped

backend-helper-display-8b47045eb7 = cancelled

backend-helper-display-a3db0ebabc = keep ownership changed; retained

backend-helper-display-b26048e0e2 = Create-only recovery needs GNU ln or Perl

backend-helper-display-bcc9225eef = Unsupported resource helper platform

backend-helper-display-c1a950664d = Input exceeds 128 MiB

backend-helper-display-d353a99eb4 = Cancelled

backend-helper-display-f0c58d1c8d = hard resource limit not confirmed

backend-helper-display-f29ce072ac = File comparison requires md5sum (Linux/WSL) or md5 (macOS)

backend-helper-display-f7df6fb542 = History investigation cancelled

backend-helper-display-f833fa6387 = invalid keep hash

backend-native-fallback-08b2647385 = ambiguous upstream short name

backend-native-fallback-1b16d9cfdb = symbolic reference requires Git

backend-native-fallback-3369979818 = feature disabled

backend-native-fallback-35c9ca76bd = tracking graph exceeds native bound

backend-native-fallback-850e897a1e = unsupported upstream namespace

backend-native-fallback-bd13f61f25 = local upstream requires Git

backend-native-fallback-c1ee10596f = remote display requires Git

backend-native-fallback-c3d2b99ca7 = legacy or rewritten remote

backend-native-fallback-d81c296d75 = remote has no fetch URL

backend-native-fallback-e1e6d89f7b = non-commit branch requires Git
