# Live forge fixture validation

Tested commit `661be4398e55117cad723ecb0962c6b86f1c1270`, tree `c97e26153d26d5b710b1c5aa074baff77745728c`. Only the test-only `src/forge/gitea/live.rs` changes from published `38a0107db2f3b601abf6d114e832d4479be25f9b`. Production authentication policy and application sources are unchanged.

The HTTP loopback fixture now uses repository-local, path-sensitive Git credentials for its exact fork URL. Checkout and push retain ordinary credential-manager behavior and the server branch OID assertion. Transient sign-in remains HTTPS-only. A fixed diagnostic prevents helper-installation failures from printing the throwaway token.

[Full results](full-results.json) record one fresh successful 16-gate run. Library: 4,997 passed, zero failed, 24 ignored, three filtered; each filtered case separately passed. Recording 3+1 overlaps library coverage. OS/checkpoint/standalone/MCP: 24/1/3/6; main and doctests: 0 and 0+0. The two exact owned ACL metadata cases run natively; other runtime suites use PID/proc/dev isolation. Real supplementary-group branch coverage remains conditional. Thirty-four tracked-source boundary observations cover the exact source; continuous filesystem immutability is not claimed.

[Live results](matrix-results.json) record six passes on each pair: Forgejo 17.0.1 / Gitea 28.3.0 and Forgejo 15.0.11 / Gitea 1.26.4. These run sequentially after the full run, against its hashed library executable, with fresh owned server directories inside PID/proc/dev isolation. Server binaries match the checksums pinned in `ci/forge-server.sh`. Tokens and server configuration files are not archived. No production credentials are used.

[Source review](source-review.json) and [execution review](execution-review.json) bind this exact candidate and the completed results. The manifest binds all portable files and 18 compressed/raw logs. The documentation successor inherits gates after complete map equality outside this archive prefix; no new runtime execution is claimed on that successor. Previous provisional runs supply no gates. Historical archives remain unchanged.
