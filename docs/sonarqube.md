# SonarQube Server and SonarQube Cloud

Issue [#203](https://github.com/JKamsker/gitcraft/issues/203), slice 1: connection and
quality gate. Gitcraft only **reads** results that SonarQube already computed. It never
runs a scanner, never uploads source code and never changes anything on the server.

## Setup

1. **Profile → Connections → SonarQube.** Choose the edition:
   - **SonarQube Server** (self-hosted, including Community Build): enter the server URL,
     including any context path, e.g. `https://sonar.example.com` or
     `https://example.com/sonarqube`. HTTPS is required. Plain HTTP is accepted only for
     this computer (`http://localhost:9000`), so a token never crosses a network
     unencrypted.
   - **SonarQube Cloud**: choose the region (`sonarcloud.io`, `sonarqube.us`,
     `sonarqube.au`) and enter the **organization key**.
2. Paste a token and choose **Save token securely**. It goes to the OS credential store
   (service `gitcraft-analysis`) and is not saved in the profile. **Check connection**
   reads the server version and validates the token.
3. Open a repository. In its toolbar, click **Quality gate** and bind the repository to a
   project: the **project key** as SonarQube shows it, optional **branch names**
   (`local=sonar` pairs, e.g. `master=main`, for branches named differently on the server),
   and whether to use the **pull request's analysis** when the checked-out branch has an
   open pull request. Several repositories can be bound to the same project (a monorepo
   split, mirrors, worktrees). The binding is stored per repository on this computer.

## Token and permissions

Use a **user token** (*My Account → Security*). Gitcraft needs read access only:

| Need | Permission |
| --- | --- |
| Quality gate, branches, pull requests, analyses (`api/qualitygates/project_status`, `api/project_branches/list`, `api/project_pull_requests/list`, `api/project_analyses/search`) | **Browse** on the project (private projects) |
| "Pending" and "newest analysis failed" (`api/ce/component`) | Browse; servers that refuse it simply don't report these states |

Project analysis tokens and global analysis tokens are meant for scanners and are not
needed. A token without Browse gets *Access denied*; an unknown or invisible project gets
*Project not found*.

## What the badge says

The chip shows **Passed**, **Failed**, **Pending**, **Unavailable** or **Stale**. Its popup
shows the project, the analyzed branch or pull request, when it was analyzed, the analyzed
commit, the open commit, the failing conditions, when gitcraft last read it, and a link to
the dashboard.

**Passed is shown only when the server's analysis is proven to be of the open commit.**
Gitcraft reads the newest analysis of the branch (`api/project_analyses/search`, which
reports the analyzed `revision`) and then the gate *of that analysis*
(`project_status?analysisId=`), so the gate and the commit belong together. For a pull
request it uses the commit recorded in `api/project_pull_requests/list`. Then:

- the analysis is of another commit (you committed or pulled since, or Sonar analyzed the
  pull request's merge commit) → **Stale**, with both commits and what the server reported;
- the server records no commit for the analysis → **Stale**;
- the newest analysis attempt failed → **Stale**;
- an analysis is queued or running for that branch or pull request → **Pending**;
- the branch or pull request was never analyzed, has no quality gate, or the status is
  unknown → **Unavailable**. Another branch's result is never shown in its place;
- any error (unreachable, timeout, refused token, rate limited) → **Unavailable**, with the
  reason. Gitcraft keeps working normally without SonarQube.

Results are read in the background, never on the UI thread: when the tab shows, when the
branch, pull request or HEAD changes, every 5 minutes, and on **Refresh**. While an analysis
stays pending, and after a failure, reads back off exponentially (30 s, 60 s, 120 s, … at
most 10 minutes). After a failure nothing is read before that time, nor before the
server's `Retry-After` (held to 1–60 minutes), even for another branch or commit; only
**Refresh** reads earlier. Offline mode and metered connections pause the background reads
(a **Refresh** still works on a metered connection). Each request times out after 15
seconds.

"The newest analysis attempt failed" relies on `api/ce/component`'s `current` task, which
is the project's single most recent finished task, not one per branch: if another branch
was analyzed after a failed attempt on yours, the failure is not seen. That reason is a
best-effort extra; the commit match above is what keeps an old result from passing.

## Server and Cloud differences

| | SonarQube Server | SonarQube Cloud |
| --- | --- | --- |
| Address | your server URL (context path allowed) | regional host |
| Organization | none | required; gitcraft checks the project belongs to it (`api/components/show`), since Cloud project keys are global |
| Authentication | `Authorization: Bearer` from 10.0; token as Basic login with empty password before 10.0 (chosen from `api/server/version`) | `Authorization: Bearer` |
| Oldest supported | 7.7 (branch and PR parameters of `project_status`) | always current |
| Branches / pull requests | Developer edition and above; Community Build analyzes the main branch only (pull requests show *not supported by this edition*) | all plans |
| Rate limits | rare | HTTP 429: gitcraft waits as long as `Retry-After` says (at least a minute) |

## Security

- The token is only in the OS credential store and in the request closure of one refresh.
  Errors are fixed sentences: no server text, URL or token reaches the UI or logs.
- Exported profile settings contain the address, organization and a credential reference,
  never the token. Re-enter the token on another computer.
- A saved token is stored for one address and organization. To switch servers, choose
  **Forget token** first: editing the address leaves the old entry in the credential store
  (gitcraft does not delete it while you type, since a corrected typo would lose the token).
- HTTPS requests to another machine use gitcraft's proxy selection (`HTTPS_PROXY`,
  `NO_PROXY`, …). Requests to this computer (`localhost`, loopback addresses) and plain
  HTTP never go through a proxy, which would see the token unencrypted and resolve
  `localhost` to itself. Certificates come from the system's store, so a company CA
  trusted by the OS is trusted here. Redirects are
  refused so that the token cannot be replayed to another host: configure the server's
  final address.
- Only `GET` requests of the endpoints above are sent.

## API references

- SonarQube Server Web API: <https://docs.sonarsource.com/sonarqube-server/latest/extension-guide/web-api/>
  (the per-version reference is in each server's *Help → Web API*, `api/webservices/list`).
- SonarQube Server 9.9 (Basic token authentication): <https://docs.sonarsource.com/sonarqube/9.9/extension-guide/web-api/>
- SonarQube Cloud Web API: <https://docs.sonarsource.com/sonarqube-cloud/advanced-setup/web-api/>
  (per-endpoint reference at <https://sonarcloud.io/web_api>).

## Not yet

Later slices of #203: new-code metrics and a findings browser, inline diff annotations,
caching across restarts.
