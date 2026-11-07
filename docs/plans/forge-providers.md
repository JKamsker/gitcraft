# Forge provider implementation plan

Base: accepted combined application `d18210503b376c77e76c6e4825e24b182b1f7eeb`.
Scope: the two eligible named ideas from issue #43, in this order.

## 1. Full GitLab merge requests

Use the existing captured Profile/glab transport and central review page. Read the MR,
project merge policy, current viewer, approvals, commits, checks and paged discussions.
Keep comments anchored to the complete GitLab diff revision. Verify a coherent head
across reads and require the same viewer/repository/head again before review writes.
Map native discussion reply/resolution and SHA-guarded approval/merge operations.
Show GitLab-specific blockers without treating unknown permissions as permission.
Expose supported native actions; do not present GitHub-only pending review,
request-changes, dismissal or suggestion-commit semantics as GitLab features.

Use only owned fake glab fixtures for reads and explicit action writes, plus local Git
repositories/UI tests. Cover source/head/account changes, discussion pagination,
removed lines, missing optional data, permission/blocker failures and JSON stdin.
Preserve a runnable, pushed application cut before starting the next named feature.

## 2. Azure DevOps and Bitbucket

Extend detection, URLs, provider lists and full central review pages for Azure DevOps
and Bitbucket Cloud. Implement provider-native conversation/line review and vote or
approval semantics, with explicit capabilities for unsupported actions. Reuse the
shared review UI, background dispatch, pinned account context and safe HTTP utilities.
Fresh local authentication selection/verification must be explicit and fail closed;
portable setup retains only public host/account expectations and pending setup needs.
Never serialize tokens, credential-helper results or auth folders into setup exports.

Use owned fake CLI/loopback HTTP fixtures only. Cover URL identities, host/account/head
pinning, paged responses, inline anchors, write payloads, stale results and actual UI
interaction. Do not call real accounts, create remote PRs or perform provider writes.

## Preservation and validation

Each coherent source unit and fix is committed with the repository-required Jonas
Kamsker identity, pushed and remote-SHA verified before its checks or the next unit.
Use a private Cargo target, two jobs and disabled incremental compilation. Run focused
checks first, then fmt, strict all-target Clippy and the complete default test suite on
immutable pushed source. Preserve exact candidate/tree/log/test-name evidence and
request independent adversarial review. No CI use, dispatch, polling or waiting;
only the parent publishes the eventual single consolidated PR.
