# GitHub repository browser (#231)

Home → Browse GitHub complements manual URL cloning. My repositories and Starred
use the profile's selected github.com account, resolved and verified by gh on a
worker. Public repository and owner searches use anonymous HTTP, including when
an account is selected. Choose an owner search result to list its public repos.
Configure the account in Connection settings; GitCraft never switches gh's global
account. Account lists currently support github.com, not Enterprise hosts.

Search is submitted explicitly or with Enter. The separate filter applies only
to loaded results. Each request fetches 50 items. Load more retains earlier rows
on failure, deduplicates stable IDs, and stops on non-advancing results. Search
incompleteness and the 1,000-result ceiling are shown. Other lists also stop at
1,000 loaded results. Requests have a 20-second HTTP/CLI deadline, a 2 MiB body
limit, no redirects in anonymous HTTP, and validated same-query pagination links.
Token resolution and account verification have their existing bounded deadlines.
Workers never sleep through a rate limit; the dialog reports the retry timestamp.

The dialog belongs to its viewport. A query replacement drops the old receiver
and cancellation scope. Results are additionally checked against viewport,
request serial, connection generation, and query. Every connection transition
invalidates the session, including A → B → A. Closing drops pending work; private
results are never serialized. Failed account verification clears private rows.

Clone hands HTTPS/SSH addresses to the existing clone form and backend, with its
folder/name, shallow/sparse options, authentication retries, progress and cancel.
Editing the URL leaves discovery transport selection. Switching transport clears
manually entered retry credentials. Open after clone defaults on; when disabled,
success remembers the path without opening a repository tab. Completion uses the
initiating viewport as before.

Local matching reads only known open, recent and workspace paths, on a bounded
worker. It examines at most 128 distinct local paths and starts no further reads
after ten seconds; an in-flight Git read retains the existing Git deadline.
It neither recursively scans nor fetches/connects. Every remote name is eligible,
and Hosting's identity comparison accepts equivalent HTTPS/SSH URLs. A chooser
handles multiple matches. Open locally rereads remotes and validates the root
before handing it to the normal repository-opening coordinator. Local results
are a snapshot until the dialog is reopened. Existing occupied-destination
protection in the clone flow remains in effect.

Architecture ceilings stay unchanged: app 17 dependencies/58 exports, forge
5/165, runtime 5/101. Discovery is a separate capability from repository-bound
Provider. Forge identity/provider selection and Home error presentation were
extracted into cohesive modules before growing their parent files. The formerly
public deployment-read cap is now private to its sole consuming module, keeping
the discovery capability within the forge export ceiling.

Focused regression coverage:

- `forge::github::discovery`: categories, escaping, visibility, statuses, rate
  limits, size rejection, search metadata and safe next-page validation.
- `forge::cli::discovery`: status/header retention on nonzero HTTP responses.
- `forge::discovery::public_http`: anonymous requests omit authorization/cookies.
- `connections::discovery_tests`: selected token verification and account mismatch.
- `app::repo_browser`: request retirement, stale viewport replies, disconnected
  workers, invalid sessions, pagination/deduplication, English/German controls,
  keyboard Close, local identities/stale paths and typed clone handoff.
- `app::clone::source`: transport changes preserve destination/name and clear
  retry credentials; open-after choice survives transport changes.
- `app::clone::localization_tests` and `app::home`: existing form/Home regressions.
- `app::windows::tests::clone_completion_without_open_preserves_owner_home_and_remembers_path`
  and the existing recorded completion/credential ownership regression.

No live GitHub calls, real clone, Windows suite or forge-server suite are required
by these focused tests. Independent adversarial review and final CI belong to the
parent's review handoff protocol before merge.
