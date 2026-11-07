# Gitcraft implementation progress

20 of 72 eligible named features integrated. Another two passed independent review and are queued to merge. 41 features remain queued.

Latest verified combined branch: [`2db60b3`](https://github.com/JKamsker/gitcraft/tree/2db60b38b9e79f35c6158d81ca20be41c4427852). Local formatting, strict Clippy and 911 tests pass; three baseline tests require external services or the system credential store and remain ignored in default runs. All 914 registered test names are accounted for.

CI is deferred until implementation, final history assembly, local checks and independent adversarial review are complete. Exactly one final PR; none created yet. Every completed source and review unit is committed and pushed. Final commits use Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>, matching `028242cd4bc3`.

| Issue | Feature | Status |
| --- | --- | --- |
| [#32](https://github.com/JKamsker/gitcraft/issues/32) | Rebase weather (M). | Integrated; local combined checks pass |
| [#32](https://github.com/JKamsker/gitcraft/issues/32) | Push preflight with identity guard (S–M). | Local repair passed; remote repair in progress |
| [#32](https://github.com/JKamsker/gitcraft/issues/32) | Lost and found by content (S–M). | Independent review passed; queued to merge |
| [#32](https://github.com/JKamsker/gitcraft/issues/32) | Drafts of the working tree (L, opt-in). | Integrated; local combined checks pass |
| [#33](https://github.com/JKamsker/gitcraft/issues/33) | Conflicts that can wait (L). | Not started |
| [#33](https://github.com/JKamsker/gitcraft/issues/33) | Changelists (L). | Not started |
| [#33](https://github.com/JKamsker/gitcraft/issues/33) | Parallel branches in one folder (XL). | Not started |
| [#34](https://github.com/JKamsker/gitcraft/issues/34) | Same code, different machines (L). | Not started |
| [#34](https://github.com/JKamsker/gitcraft/issues/34) | Behaviour timeline (L–XL). | Not started |
| [#35](https://github.com/JKamsker/gitcraft/issues/35) | One continuous review page (M). | Not started |
| [#35](https://github.com/JKamsker/gitcraft/issues/35) | Review freshness (L). | Not started |
| [#35](https://github.com/JKamsker/gitcraft/issues/35) | Reviewer echo (M). | Not started |
| [#35](https://github.com/JKamsker/gitcraft/issues/35) | AI objections that come with a reproducer (L). | Not started |
| [#35](https://github.com/JKamsker/gitcraft/issues/35) | Review storyboard (M–L). | Not started |
| [#35](https://github.com/JKamsker/gitcraft/issues/35) | Fix survival radar (L). | Not started |
| [#35](https://github.com/JKamsker/gitcraft/issues/35) | Backport by contract (L). | Not started |
| [#35](https://github.com/JKamsker/gitcraft/issues/35) | Intent cards (M–L). | Not started |
| [#35](https://github.com/JKamsker/gitcraft/issues/35) | Release choreography across repositories (XL). | Not started |
| [#36](https://github.com/JKamsker/gitcraft/issues/36) | Saved views of the graph (M). | Independent review passed; queued to merge |
| [#36](https://github.com/JKamsker/gitcraft/issues/36) | Line dossier: why is this line like this? (M–L). | In progress |
| [#36](https://github.com/JKamsker/gitcraft/issues/36) | Symbol biography (L). | Assigned; implementation pending |
| [#36](https://github.com/JKamsker/gitcraft/issues/36) | What usually comes with this? (M). | Assigned; implementation pending |
| [#37](https://github.com/JKamsker/gitcraft/issues/37) | Clone options: shallow, partial and sparse (S–M). | Integrated; local combined checks pass |
| [#37](https://github.com/JKamsker/gitcraft/issues/37) | Bundles (S–M). | Integrated; local combined checks pass |
| [#37](https://github.com/JKamsker/gitcraft/issues/37) | Scrub history with filter-repo (L). | Not started |
| [#38](https://github.com/JKamsker/gitcraft/issues/38) | A graph that reads better (S). | Integrated; local combined checks pass |
| [#38](https://github.com/JKamsker/gitcraft/issues/38) | Keyboard-first staging and moving between panes (M). | Integrated; local combined checks pass |
| [#38](https://github.com/JKamsker/gitcraft/issues/38) | Motion that explains what changed (M–L). | Not started |
| [#39](https://github.com/JKamsker/gitcraft/issues/39) | Contrast | Integrated; local combined checks pass |
| [#39](https://github.com/JKamsker/gitcraft/issues/39) | A graph you can hear | Not started |
| [#40](https://github.com/JKamsker/gitcraft/issues/40) | Structural diff for refactors | Integrated; local combined checks pass |
| [#40](https://github.com/JKamsker/gitcraft/issues/40) | File migration map | Integrated; local combined checks pass |
| [#40](https://github.com/JKamsker/gitcraft/issues/40) | Encrypted files | Integrated; local combined checks pass |
| [#42](https://github.com/JKamsker/gitcraft/issues/42) | Gerrit | Integrated; local combined checks pass |
| [#42](https://github.com/JKamsker/gitcraft/issues/42) | Patches by email | Not started |
| [#43](https://github.com/JKamsker/gitcraft/issues/43) | Run an Actions workflow locally | Not started |
| [#43](https://github.com/JKamsker/gitcraft/issues/43) | Full GitLab merge requests | In progress |
| [#43](https://github.com/JKamsker/gitcraft/issues/43) | Azure DevOps and Bitbucket | Assigned; implementation pending |
| [#43](https://github.com/JKamsker/gitcraft/issues/43) | Each profile with its own SSH key and forge account | Integrated; local combined checks pass |
| [#43](https://github.com/JKamsker/gitcraft/issues/43) | SSH key helper | Integrated; local combined checks pass |
| [#43](https://github.com/JKamsker/gitcraft/issues/43) | Dev containers | Not started |
| [#44](https://github.com/JKamsker/gitcraft/issues/44) | gitcraft as git difftool / mergetool, and comparing any two files | Fixing an adversarial review finding |
| [#45](https://github.com/JKamsker/gitcraft/issues/45) | Practice repositories and a glossary | Integrated; local combined checks pass |
| [#45](https://github.com/JKamsker/gitcraft/issues/45) | A tour of this repository | Integrated; local combined checks pass |
| [#45](https://github.com/JKamsker/gitcraft/issues/45) | New machine, same setup | Integrated; local combined checks pass |
| [#46](https://github.com/JKamsker/gitcraft/issues/46) | Lanes by branching model | Not started |
| [#46](https://github.com/JKamsker/gitcraft/issues/46) | History minimap | Not started |
| [#46](https://github.com/JKamsker/gitcraft/issues/46) | Activity calendar | Integrated; local combined checks pass |
| [#46](https://github.com/JKamsker/gitcraft/issues/46) | Churn and contributor atlas | Not started |
| [#46](https://github.com/JKamsker/gitcraft/issues/46) | Watch the repository grow | Not started |
| [#46](https://github.com/JKamsker/gitcraft/issues/46) | Contributor statistics | Integrated; local combined checks pass |
| [#46](https://github.com/JKamsker/gitcraft/issues/46) | Quiet celebrations | Not started |
| [#46](https://github.com/JKamsker/gitcraft/issues/46) | History postcards | Not started |
| [#47](https://github.com/JKamsker/gitcraft/issues/47) | Connection doctor and proxy page | Not started |
| [#47](https://github.com/JKamsker/gitcraft/issues/47) | A tray icon that keeps watching | Not started |
| [#47](https://github.com/JKamsker/gitcraft/issues/47) | HTTPS sign-ins page | Not started |
| [#47](https://github.com/JKamsker/gitcraft/issues/47) | Localization | Not started |
| [#48](https://github.com/JKamsker/gitcraft/issues/48) | The prompt behind a commit | Not started |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | Pull from one remote, push to another | Not started |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | Publish submodule changes before their parent | Not started |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | Submodule conflicts as graphs | Not started |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | Remote refspec editor | Not started |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | Publish to several hosts | Not started |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | Push outbox | Not started |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | Subtrees | Not started |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | Shared and borrowed objects | In progress |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | All refs | Not started |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | Replace refs and grafts | Not started |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | How an object is stored | In progress |
| [#49](https://github.com/JKamsker/gitcraft/issues/49) | Reftable: see it and migrate to it | Not started |
| [#50](https://github.com/JKamsker/gitcraft/issues/50) | Three-state staging lens | Integrated; local combined checks pass |
| [#50](https://github.com/JKamsker/gitcraft/issues/50) | File versions across branches | Integrated; local combined checks pass |

The initial recovery ZIP is an older immutable snapshot. Current source and review branches contain newer work; the adjacent latest-session-state directory records their checkpoints.
