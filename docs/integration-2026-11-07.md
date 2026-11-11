# Integration of open pull requests — 2026-11-07

Target: `JKamsker/gitcraft:dev` at `74027116e8f20334716a6b1208717bb33c9c6280`.

This integration preserves all 100 open PR heads captured at the start of the work. Dependent PRs are included through their stack tips; merge commits preserve the complete source history. The source PRs can be closed as superseded after the integration checks pass.

The integration combines shared interfaces for file menus (links, tools, history, stashing and LFS), whole-file selection and search, rich diff renderers, remote execution lanes, and the persistent timeline. Staged-change folding and generated-file folding have separate modules. MCP operation attribution is carried into the timeline, and multi-step operations record their recovery actions there.

## Source heads

| PR | Change | Captured head |
| --- | --- | --- |
| [#52](https://github.com/JKamsker/gitcraft/pull/52) | feat: fold staged changes into an older commit | `553e5f96bbeab82304f91a69b0aba00ee53a999a` |
| [#54](https://github.com/JKamsker/gitcraft/pull/54) | feat: reflog browser and lost-commit recovery | `527a4f66e0943f41b1f033f0df9790b922303ff6` |
| [#55](https://github.com/JKamsker/gitcraft/pull/55) | feat: say what a destructive operation will do before it runs | `d9edb82a51e20d9543ade52bc5489673a7da5104` |
| [#56](https://github.com/JKamsker/gitcraft/pull/56) | feat: bisect from a commit's menu, with a banner, graph marks and bisect run | `27a9f842a3783cc3aa9e1045181bedebd3c88983` |
| [#57](https://github.com/JKamsker/gitcraft/pull/57) | feat: a refused push opens a banner with pull-then-push and force push with lease, and Sync | `75939f588610952c5f7b8026b7da4ccd342b6007` |
| [#59](https://github.com/JKamsker/gitcraft/pull/59) | feat: conflict warning before merge, rebase or cherry-pick | `75b8ae8ade425b3905cf1727f1cc8e17166c2a14` |
| [#60](https://github.com/JKamsker/gitcraft/pull/60) | feat: what you typed survives a crash or a restart | `4dc3fe79e83cb0a11dab33360106655a1c0a5b24` |
| [#61](https://github.com/JKamsker/gitcraft/pull/61) | feat: drag and drop in the graph and the sidebar | `5a0fc78f43a5eb7a970f7eb447bf1a0aed59e9d4` |
| [#62](https://github.com/JKamsker/gitcraft/pull/62) | feat: commit templates, Conventional Commits, co-authors, prepare-commit-msg and completion | `d52e469e3fe73b42efb189e5ecb5ab3bbf653ced` |
| [#63](https://github.com/JKamsker/gitcraft/pull/63) | feat: each conflict's original, and keeping both versions of a binary conflict | `dc1143f82232ac8bd9206841a8199ae02bbfa316` |
| [#64](https://github.com/JKamsker/gitcraft/pull/64) | feat: teleport a tab's work to another machine | `d0dd5de78a200e15639348788f96f6ad3f2e966f` |
| [#65](https://github.com/JKamsker/gitcraft/pull/65) | feat: find in a diff, a file or blame (Ctrl+F) | `d1e4e15bcf57da30d051ff8d0d9d0bff21cd5a02` |
| [#66](https://github.com/JKamsker/gitcraft/pull/66) | feat: visible keyboard focus, screen-reader names and reduced motion | `f861eb0fa63651934048225129c6384240a37b2c` |
| [#67](https://github.com/JKamsker/gitcraft/pull/67) | feat: AI commit message and branch review through a CLI you already have | `a47dc91480f4ff2db872d8918f36254e56cc281e` |
| [#68](https://github.com/JKamsker/gitcraft/pull/68) | feat: a second start goes to the window that's already open | `44e76cc898e9767b71565dd86689ddfd132e8b4e` |
| [#69](https://github.com/JKamsker/gitcraft/pull/69) | feat: custom commands in menus and the palette | `f11414836dc0bba10c6bcdffc03e8b5ca15c1a23` |
| [#70](https://github.com/JKamsker/gitcraft/pull/70) | feat: external diff and merge tools (Open in diff tool, Resolve in merge tool) | `3db48c4c49e2af9e23436f86d80c27336c54f77f` |
| [#71](https://github.com/JKamsker/gitcraft/pull/71) | feat: portable mode, with a gitcraft.portable file or a data folder next to gitcraft | `cfa9a2da3e30bc7624eba295e9876f6249f2a563` |
| [#72](https://github.com/JKamsker/gitcraft/pull/72) | feat: copy a link, or open it on the forge | `e919bf3ad955fde4ea38d74ef2b393d0ef3c8570` |
| [#73](https://github.com/JKamsker/gitcraft/pull/73) | feat: search operators in the graph's search field | `2c08c741191acdc662d933ac18dec63360bbfc24` |
| [#74](https://github.com/JKamsker/gitcraft/pull/74) | feat: signature, contained in, notes and backlinks in a commit's details | `1086e9193bc76be6af3e8415ec288749794ac5da` |
| [#75](https://github.com/JKamsker/gitcraft/pull/75) | feat: keys for the main actions and the tabs, shown in tooltips, the palette and menus | `411fd64d0eb564b5ede4048122707d5afb04ce50` |
| [#76](https://github.com/JKamsker/gitcraft/pull/76) | feat: remember conflict resolutions (git rerere) and show what was reused | `da8865dc839207756a67b112bc064d0872baa76c` |
| [#77](https://github.com/JKamsker/gitcraft/pull/77) | feat: progress and Cancel for fetch, pull, push and clone; actions asked for meanwhile wait | `414796a8dccc6f68f8ca725836098e5004a93df5` |
| [#78](https://github.com/JKamsker/gitcraft/pull/78) | feat: a ranked, selection-aware command palette with a second step for branches | `41f532adb5e4eaf71667898b1caa94a5295131d2` |
| [#79](https://github.com/JKamsker/gitcraft/pull/79) | feat(diff): moved code marked, with its two places shown together | `83cbfe8fe680d6a724a77ae27f03a9bab12ceda3` |
| [#80](https://github.com/JKamsker/gitcraft/pull/80) | feat: stash picked files, only what is staged, or a snapshot | `4b1c4c9d7cffb1fd3b83b36a35ec6fc338947877` |
| [#81](https://github.com/JKamsker/gitcraft/pull/81) | feat: a "weakens checks" badge on files and hunks that remove assertions, skip tests or loosen CI | `66aef84c5b206f53b14f95ae9bf283152c562bba` |
| [#82](https://github.com/JKamsker/gitcraft/pull/82) | feat: What this merge resolved (remerge diff) on merge commits | `679b6ea12d5d915308db712fea4a2ee937dcd2e3` |
| [#83](https://github.com/JKamsker/gitcraft/pull/83) | feat: drag handles, P/R/E/S/F/D keys and an Edit step in the interactive rebase list | `7b5844afe71adb95395e84295e52e7503dd3f43b` |
| [#84](https://github.com/JKamsker/gitcraft/pull/84) | feat: graph order and first parent, reset --keep, detached HEAD banner, skip hooks once | `e968181562992fcde63becea1c56b8bbd799768a` |
| [#85](https://github.com/JKamsker/gitcraft/pull/85) | feat: stacked branches: "This branch is based on…" and the STACKS section | `11e5e80b33ef8b96b0a7f81ef6432adf809bfd67` |
| [#86](https://github.com/JKamsker/gitcraft/pull/86) | feat: an update keeps the version it replaced; go back to it and stay there | `7a3e2257600ac74898ae46b43aa5c1eb2316e17a` |
| [#87](https://github.com/JKamsker/gitcraft/pull/87) | feat: branch cleanup, pinned branches and sort by last commit | `c0034b52d4382ade56dc9d0e6334a005d5e85a82` |
| [#88](https://github.com/JKamsker/gitcraft/pull/88) | feat: Git LFS file locks: padlocks on file rows, Lock / Unlock, a locks panel, warnings and unlock after push | `2167bfb752bed466fd8a6af347a32b9ed2bf8cba` |
| [#89](https://github.com/JKamsker/gitcraft/pull/89) | feat: parallel bisect on a bigger machine: k commits at once in worktrees, here, in WSL or over SSH | `1e6bfdee5967615e7f001569b34d229b9f00920e` |
| [#90](https://github.com/JKamsker/gitcraft/pull/90) | feat: pop the diff, terminal, pull request and CI view out into their own windows | `0f8d259a62aa5ed529518320c37655cc7ec39df4` |
| [#91](https://github.com/JKamsker/gitcraft/pull/91) | feat: conflict archaeology: each side's history of the file, and where the conflict started | `0995f9426c040ab2e8bcba42f54f72c811480e61` |
| [#92](https://github.com/JKamsker/gitcraft/pull/92) | feat: gitcraft as an MCP server, with agents' changes in the window's undo list | `01e48badf666420b82e08cdd9033930329a4bef7` |
| [#93](https://github.com/JKamsker/gitcraft/pull/93) | feat: repository doctor — which configuration wins, and hidden changes | `150857e9e8e609dbc41fc555d4991b70cb90f65b` |
| [#94](https://github.com/JKamsker/gitcraft/pull/94) | feat: agent tasks in worktrees of their own, and a setup recipe for every new worktree | `570e8ce27652697b01ae034a83464778221cc9b3` |
| [#95](https://github.com/JKamsker/gitcraft/pull/95) | feat: explain a git error, with a button that fixes it | `dfaa313892efbbba90a9a5951c193af53e45373f` |
| [#96](https://github.com/JKamsker/gitcraft/pull/96) | feat: DCO sign-off toggle, unsigned commits flagged, sign off a branch's commits | `1ab179931516147fdd885e3250107a293770ad8c` |
| [#97](https://github.com/JKamsker/gitcraft/pull/97) | feat: ghost run, the result of an operation drawn on the graph before it is done | `54b2ffec777d99dcaeb00ebaa6c6e35fa29ce7d0` |
| [#98](https://github.com/JKamsker/gitcraft/pull/98) | feat: first steps for a new repository, a "Did you know" line and first-time hints | `4525a715bcfa880658f400894ad2d156b3e52cc4` |
| [#99](https://github.com/JKamsker/gitcraft/pull/99) | feat: lazy tabs: restored tabs read when first shown, idle ones are suspended | `d62f6a6794e21827fa60dc5f605760cb0314a1bf` |
| [#100](https://github.com/JKamsker/gitcraft/pull/100) | feat: a collision matrix of worktree branches and the order to land them in | `e2656f46eca8d1726fbfc8d0c82e22993a29c32a` |
| [#101](https://github.com/JKamsker/gitcraft/pull/101) | feat: split a commit into two or more, each with its own message | `2534e1fe9af83e21a468ae5d991687913cbf98e8` |
| [#102](https://github.com/JKamsker/gitcraft/pull/102) | feat: a diff below the graph, and back / forward through the recent views | `2c73977d22c4ebecec57716dd129ded96956c2c3` |
| [#103](https://github.com/JKamsker/gitcraft/pull/103) | feat: workspaces on the start page, with each repository's state and fetch / open all | `0b58db50e65fd03e4809e13db961db6030cfae6d` |
| [#104](https://github.com/JKamsker/gitcraft/pull/104) | feat: a start page that does more (filter, recent status, drag and drop, folder scan) | `d4e9f2e3fc374837c5205c9d1b2aba1070a6e9d5` |
| [#105](https://github.com/JKamsker/gitcraft/pull/105) | feat: an Activity panel with each operation's commands, exit codes, times and full output | `5de8228043fe0e313e5e244d0ff1dc26e9ac8ee4` |
| [#106](https://github.com/JKamsker/gitcraft/pull/106) | feat: LFS storage: disk used, files that are only pointers, download, what to fetch, prune, track in LFS | `298f28bb56a6cf64f0262544e945d4bb50ada4d4` |
| [#107](https://github.com/JKamsker/gitcraft/pull/107) | feat(diff): unchanged lines between hunks fold, opening up, down or by function | `ffb0b2c6dede4619a6c706d372b05ce3118d7185` |
| [#108](https://github.com/JKamsker/gitcraft/pull/108) | feat: start work from an issue assigned to you | `43d256780bc9be0035093ed0dbebb5c5d1dd962e` |
| [#109](https://github.com/JKamsker/gitcraft/pull/109) | feat: compare versions with range-diff — before / after rewrites, PRs since last looked at | `68a7d5ed028afe007f85b5157e9976b59f202e98` |
| [#110](https://github.com/JKamsker/gitcraft/pull/110) | feat: history of selected lines, and reverting a file, hunk or lines of a past commit | `6f38f6491520ea920ffbea56ac7749b8f36efb85` |
| [#111](https://github.com/JKamsker/gitcraft/pull/111) | feat: test lane, a pass / fail square per commit for a test command, kept by tree as git-test keeps it | `3d837a7c33fa4bab0881ff35cec4ff4838806c9f` |
| [#112](https://github.com/JKamsker/gitcraft/pull/112) | feat: offline mode, and background work paused on a metered connection or without one | `e6d34653456f8be57a5259797a932d180c6f8ffb` |
| [#113](https://github.com/JKamsker/gitcraft/pull/113) | feat: gitcraft log/blame/show/compare from the terminal, and History/Blame in the file managers | `07ba1bc9d705017aaf9dafe84d2d3512d663c6cb` |
| [#114](https://github.com/JKamsker/gitcraft/pull/114) | feat: repository doctor — ignore rules and attributes | `4eff5129a8d66820ea564272d1864787aa059ac6` |
| [#115](https://github.com/JKamsker/gitcraft/pull/115) | feat: bake-off between attempts, and assembling a branch from their best parts | `4988f26898b63b8636e18581fb321e0afe05a82a` |
| [#116](https://github.com/JKamsker/gitcraft/pull/116) | feat: a timeline kept across restarts, as an undo tree (Ctrl+Shift+Z) | `14aec0f897acecae6d2dd717d3cc34b1e8bd51f6` |
| [#117](https://github.com/JKamsker/gitcraft/pull/117) | feat: restack dependent branches: preview, conflict stop, continue / abort, one undo, --update-refs | `393531995db097d71c11c36d81e3ca19ee87fa97` |
| [#118](https://github.com/JKamsker/gitcraft/pull/118) | feat: "Where's my work?", a panel of what exists only in one repository, on every machine | `a9951371d3248979382cd33e197a170ff7c85a2a` |
| [#119](https://github.com/JKamsker/gitcraft/pull/119) | feat(diff): prose mode (wrapped paragraphs, word changes) and a frame for rich diff views | `bb574f394cd675060164c81f86a58631a08387eb` |
| [#120](https://github.com/JKamsker/gitcraft/pull/120) | feat: CODEOWNERS owners on file rows, who a change needs, reviewer suggestions, check | `573a6fb0eaaad319af294412a4fbc3f706c266fe` |
| [#121](https://github.com/JKamsker/gitcraft/pull/121) | feat: taskbar progress for clone, fetch, pull and push, and a menu of recent repositories | `e9cee857dde84ae0405aa23de7de8ee474cc3109` |
| [#122](https://github.com/JKamsker/gitcraft/pull/122) | feat: conflict decisions that remember why (note, test, stale detection), kept in git | `0f04e3fe8c23878cc79cc10d0ea5590ed3e746e9` |
| [#123](https://github.com/JKamsker/gitcraft/pull/123) | feat: an agent fence: paths agents must stay out of, written into their configs and checked | `995e1cd230b578a657e5f313a1ac0e5af77cffbf` |
| [#124](https://github.com/JKamsker/gitcraft/pull/124) | feat: move changes between commits, or back to the working changes | `caba291c41f546210886615ffba18835004d8100` |
| [#125](https://github.com/JKamsker/gitcraft/pull/125) | feat: a pull-request inbox across every repository and forge on the start page | `cde9c6bc78428950a36d86860414d2f8f291ebda` |
| [#126](https://github.com/JKamsker/gitcraft/pull/126) | feat: Unity projects: set-up checks and fixes, .meta pairing, UnityYAMLMerge registration and solve | `1693f162f6caa5e6b00fc5651d151a0d0e00b1f8` |
| [#127](https://github.com/JKamsker/gitcraft/pull/127) | feat: a scope drift meter for worktrees: what changed beyond the task, item by item | `56352ce58d35bcc8157848d8ee864849d2fedabf` |
| [#128](https://github.com/JKamsker/gitcraft/pull/128) | feat: choose the diff algorithm, fold generated files and lockfiles | `d4050850eab47a2b639375740c38cb650074aaf4` |
| [#129](https://github.com/JKamsker/gitcraft/pull/129) | feat: gitcraft:// links and pasted forge links open in your clone, or offer to clone | `f1d88979abc35cde282da70cfdc3761bad239457` |
| [#130](https://github.com/JKamsker/gitcraft/pull/130) | feat: repository doctor — hooks | `8cab45f55b338954fa26b5aec1de9500320e6b61` |
| [#131](https://github.com/JKamsker/gitcraft/pull/131) | feat: a receipt for each rewrite: files, authors, messages, signatures, commit by commit | `af5f0c141b84a0146dbebe2281702a2c052b6f29` |
| [#132](https://github.com/JKamsker/gitcraft/pull/132) | feat: agent turns as review steps, saved by gitcraft checkpoint | `6694175a24e2d86d2dfb71a1f0a2821d0ba8d12a` |
| [#133](https://github.com/JKamsker/gitcraft/pull/133) | feat: rehearse a rebase or merge in a scratch worktree, tests included, then do it for real | `c7fb6c34902b7b79671c52975fd149ee0e5bdc85` |
| [#134](https://github.com/JKamsker/gitcraft/pull/134) | feat: distribute staged fixes into the commits they belong to (absorb) | `f6f3946a332d7b458b45a24c35440bd05ef941e5` |
| [#135](https://github.com/JKamsker/gitcraft/pull/135) | feat(diff): config files by key (JSON, YAML, TOML) | `e07e0ba3e234699c4479b572566046b6b2032f63` |
| [#136](https://github.com/JKamsker/gitcraft/pull/136) | feat: binary files in diffs: kind, sizes and details, document text, archive members, hex | `c404bdd72dbef75b52e1bd64ac6016256da358e1` |
| [#137](https://github.com/JKamsker/gitcraft/pull/137) | feat: pull requests from forks: push to the contributor's branch, rebase and push, badges | `65b037d865c16b23e214851571cdbfc6aab3e287` |
| [#138](https://github.com/JKamsker/gitcraft/pull/138) | feat: find the interaction, the commits of two branches that fail together (ddmin) | `f48a5071542b40c90da28c12edadb669d2344d2b` |
| [#139](https://github.com/JKamsker/gitcraft/pull/139) | feat: remove one operation from the timeline and keep the work after it | `6f828d541328131471ad21fce206f91e48322bc1` |
| [#140](https://github.com/JKamsker/gitcraft/pull/140) | feat: publish and update a stack's pull requests bottom-up, retarget after a merge, on all four forges | `353ce3198a20c6f70701e66f097450b2d93e2ad7` |
| [#141](https://github.com/JKamsker/gitcraft/pull/141) | feat: repository doctor — health and maintenance | `35a589b04c2ebc5be7317474074d25ab71e53a6f` |
| [#142](https://github.com/JKamsker/gitcraft/pull/142) | feat: hand it to the agent: line notes in a tray, typed into its terminal | `162a2358561644dd7bb22821dc6be1357baf7125` |
| [#143](https://github.com/JKamsker/gitcraft/pull/143) | feat: picture diffs compare in 2-up, Swipe, Onion skin and Difference, with a history filmstrip | `7ca81d3624081faeddfe6d97a5bb0a9cb7984adc` |
| [#144](https://github.com/JKamsker/gitcraft/pull/144) | feat: Help → Copy diagnostics | `83761c365aa8d24189779844877ab36c5367c506` |
| [#145](https://github.com/JKamsker/gitcraft/pull/145) | feat: Cut release… — version, notes, manifests, tag, push and the forge's release | `72a17198a73fef8e7f5aa343ac9cb235ac3c0c30` |
| [#146](https://github.com/JKamsker/gitcraft/pull/146) | feat: an AI authorship lens: robot on agent commits, line stripes, branch figures | `6b53fbc55155c1516adc5c6aaba722cfc55e5634` |
| [#147](https://github.com/JKamsker/gitcraft/pull/147) | feat(diff): API contract changes for OpenAPI and Swagger documents | `b81f69faeade2cc379411600c64a74147c4a862f` |
| [#148](https://github.com/JKamsker/gitcraft/pull/148) | feat: Jupyter notebooks diff cell by cell, with outputs before and after and Strip outputs when committing | `35cde84c2ee85dc9b3759a6fd14c424e6545ed92` |
| [#149](https://github.com/JKamsker/gitcraft/pull/149) | feat: hotfix flow — start from a tag, finish into main and develop in one undoable step | `da23dc447962ba559b729a6c3bf2ebe77748f1f9` |
| [#150](https://github.com/JKamsker/gitcraft/pull/150) | feat: what a package will publish (cargo / npm) | `1fd400748bef3136c33cc3b80d861774f4fac2d1` |
| [#151](https://github.com/JKamsker/gitcraft/pull/151) | feat(diff): dependency changes from lockfiles | `980e0383c90ba8b46addf1ef14113a2f495cdbf2` |
| [#152](https://github.com/JKamsker/gitcraft/pull/152) | feat(diff): CSV/TSV as a grid, and .dvc files as the data they track | `f460afcfd22c39f9a3f8643a966dd52a28d18704` |
| [#156](https://github.com/JKamsker/gitcraft/pull/156) | feat: tabs in the title bar on Windows, scrolling tabs and movable app buttons | `6f3f618001c9018e990e612445d9fa5f8407550f` |
