# Object storage independent review — static unit

Candidate: `c805f6eded6e150b62e3acd44a85b109d582a1ae`; tree `f029597db8b70eb952dc68a5a0ab5848378b5050`; base `b5df68070a6df9d91cd29ca5d6270db80c9040c9`.

Scope: exact queue entries #49 Shared and borrowed objects, #49 How an object is stored. Read-only inspection of all 14 changed files, existing tests, queue requirements, AGENTS.md and CI policy. No CI use. No application source edits.

Evidence checks: all four author manifest log SHA-256 values match their files. Test-list has 884 distinct names. Full log reports 881 passed / 0 failed / 3 ignored / 0 filtered. This verifies prior evidence, not fresh execution. Source tree matches manifest. Baseline retained-name comparison remains to be independently checked.

Potential blocking finding: directory identity is not retained by Snapshot or current(). Only objects, objects/info and alternates final path components are tested for symlinks. Repository root, .git and ancestor directory replacement are not checked. detach() subsequently uses textual hard_link/read/remove_file; restore() and lock cleanup also resolve mutable textual ancestors. A replacement directory with the same snapshot bytes can pass current(), redirecting writes and alternate removal away from the originally inspected metadata. Local detach also lacks a file identity equality check before removing the original name. Need exact-source focused reproduction; no runtime claim yet.

Positive static observations: object Git requests clear repository/namespace/index/alternate environment overrides, disable lazy fetching and replacement objects; pack copy disables thin/reuse and has strict index-pack plus required-object pack coverage; post-detach failure attempts non-overwriting restore. Current tests cover normal loose/pack/SHA-256, tree/file navigation, donor removal, staleness, partial clones, selected profiles/Practice, fake SSH and WSL request paths, and UI result guards. Existing tests do not exercise root/ancestor replacement.

Status: WIP static review. Candidate not approved.
