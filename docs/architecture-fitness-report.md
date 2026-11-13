# Architecture fitness report

A snapshot taken when the gate was adopted (#209), for orientation; it is not kept up to
date. CI writes the current report to the job summary of the `rust-file-size` gate, and
`cargo run --release --manifest-path ci/arch-fitness/Cargo.toml -- report --markdown FILE`
regenerates it. See [architecture-boundaries.md](architecture-boundaries.md).

Source: `130885caf5f5d40bfb3b67ca1c277c329c53acd9` · policy `2026-11-13.1` · schema 1

16 crates, 2681 files, 3102 modules, 22035 items in 28 domains. 291 domain edges (119 forbidden by policy), 1 cycles, 246 `RepoTab` fields.

Resolution: 131827 local paths, 157643 external, 31 unresolved qualified paths, 506 unresolved simple names; 190894 method calls (not resolved: no type inference); 39236 macro bodies parsed, 488 unparsed; 8648 tab-field accesses with an unknown receiver.

## Check result

No violations.


## Domains

| Domain | Layer | Modules | Files | Items | Exported | Fan-out | Fan-in | Foreign symbols used | Own symbols used elsewhere | Test fan-out |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| agents | features | 173 | 128 | 697 | 200 | 18 | 12 | 162 | 73 | 14 |
| app | shell | 193 | 172 | 579 | 58 | 17 | 11 | 359 | 24 | 12 |
| bisect | features | 81 | 68 | 221 | 46 | 10 | 6 | 120 | 40 | 6 |
| changes | features | 106 | 82 | 297 | 88 | 14 | 7 | 243 | 49 | 10 |
| conflicts | features | 112 | 89 | 483 | 88 | 11 | 7 | 145 | 31 | 9 |
| dev | shell | 40 | 32 | 0 | 0 | 0 | 0 | 0 | 0 | 2 |
| diff | features | 220 | 208 | 912 | 90 | 11 | 10 | 214 | 62 | 12 |
| doctor | features | 81 | 70 | 207 | 34 | 8 | 12 | 247 | 17 | 8 |
| forge | services | 135 | 108 | 646 | 165 | 5 | 18 | 53 | 171 | 6 |
| git | services | 482 | 391 | 2378 | 375 | 12 | 23 | 116 | 772 | 10 |
| graph | features | 90 | 82 | 192 | 52 | 18 | 14 | 169 | 42 | 11 |
| guidance | features | 26 | 21 | 76 | 44 | 8 | 6 | 51 | 15 | 9 |
| insights | features | 147 | 138 | 774 | 293 | 15 | 1 | 149 | 5 | 8 |
| os | platform | 59 | 36 | 272 | 26 | 4 | 8 | 17 | 60 | 3 |
| release | features | 68 | 52 | 207 | 55 | 9 | 5 | 93 | 12 | 7 |
| remote | services | 67 | 59 | 372 | 69 | 6 | 23 | 60 | 106 | 7 |
| repo-core | coordinator | 320 | 294 | 432 | 91 | 24 | 20 | 608 | 102 | 20 |
| review | features | 155 | 150 | 527 | 71 | 15 | 9 | 252 | 63 | 10 |
| rewrite | features | 128 | 128 | 422 | 89 | 14 | 7 | 228 | 50 | 9 |
| runtime | platform | 41 | 35 | 220 | 101 | 5 | 25 | 19 | 87 | 5 |
| settings | services | 85 | 70 | 275 | 34 | 15 | 18 | 179 | 50 | 10 |
| sidebar | features | 37 | 30 | 86 | 31 | 19 | 6 | 129 | 20 | 7 |
| sync | features | 63 | 75 | 190 | 27 | 13 | 6 | 120 | 35 | 14 |
| terminal | features | 29 | 26 | 114 | 23 | 9 | 8 | 62 | 29 | 6 |
| tooling | shell | 26 | 24 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| ui-kit | presentation | 100 | 80 | 317 | 98 | 4 | 20 | 9 | 113 | 5 |
| update | features | 11 | 9 | 66 | 10 | 2 | 5 | 5 | 15 | 1 |
| worktrees | features | 27 | 24 | 145 | 52 | 5 | 4 | 56 | 61 | 3 |

### Cycles (strongly connected domains)

- agents ⇄ app ⇄ bisect ⇄ changes ⇄ conflicts ⇄ diff ⇄ doctor ⇄ forge ⇄ git ⇄ graph ⇄ guidance ⇄ insights ⇄ os ⇄ release ⇄ remote ⇄ repo-core ⇄ review ⇄ rewrite ⇄ runtime ⇄ settings ⇄ sidebar ⇄ sync ⇄ terminal ⇄ ui-kit ⇄ update ⇄ worktrees

### Modules with the most foreign dependencies

| Module | Domain | Foreign domains | Foreign symbols |
| --- | --- | ---: | ---: |
| `gitcraft::repo::tab_model` | repo-core | 19 | 128 |
| `gitcraft::repo::actions` | repo-core | 16 | 87 |
| `gitcraft::repo::actions::palette_actions` | repo-core | 12 | 27 |
| `gitcraft::repo::toolbar` | repo-core | 11 | 28 |
| `gitcraft::repo::messages` | repo-core | 10 | 35 |
| `gitcraft::repo::graph_view::menu` | graph | 10 | 32 |
| `gitcraft::repo::sidebar::branches` | sidebar | 10 | 31 |
| `gitcraft::repo::graph_view::menu::references` | graph | 10 | 22 |
| `gitcraft::app` | app | 10 | 21 |
| `gitcraft::repo::sidebar::worktrees` | sidebar | 9 | 35 |
| `gitcraft::repo::detail::changes` | changes | 9 | 29 |
| `gitcraft::repo::load` | repo-core | 9 | 26 |
| `gitcraft::repo::sidebar::remotes::menu` | sidebar | 9 | 24 |
| `gitcraft::repo::sidebar::branches::checkout` | sidebar | 9 | 20 |
| `gitcraft::repo::tab_construction` | repo-core | 9 | 20 |

## Forbidden-by-policy edges

| From | To | References | Symbols | First site |
| --- | --- | ---: | ---: | --- |
| agents | app | 3 | 2 | `src/mcp/mod.rs:153` gitcraft::app::lifecycle::repo_root |
| agents | conflicts | 20 | 6 | `src/mcp/tools/resolve.rs:12` gitcraft::resolver::report::MAX_ESTIMATE_MINUTES |
| agents | diff | 5 | 2 | `src/repo/agents/handoff.rs:17` gitcraft::repo::diff::DiffData |
| agents | doctor | 2 | 1 | `src/repo/agents/turns.rs:177` gitcraft::repo::repository_tools::Act |
| agents | graph | 5 | 3 | `src/repo/authorship_view.rs:122` gitcraft::repo::history_filter::rules |
| agents | review | 8 | 3 | `src/repo/agents/handoff.rs:18` gitcraft::repo::pr_review::LineTarget |
| agents | rewrite | 27 | 11 | `src/repo/agent/plan.rs:23` gitcraft::repo::quick_edit |
| agents | sidebar | 8 | 4 | `src/repo/agent/plan.rs:24` gitcraft::repo::sidebar::branch_delete::redo_delete |
| agents | terminal | 12 | 4 | `src/agent/launch.rs:14` gitcraft::terminal::shells::ShellSpec |
| agents | update | 1 | 1 | `src/mcp/server.rs:160` gitcraft::update::CURRENT |
| bisect | app | 2 | 1 | `src/parallel_bisect/bundle/native.rs:169` gitcraft::app::lifecycle::strip_verbatim |
| bisect | doctor | 3 | 1 | `src/repo/local_ci/mod.rs:222` gitcraft::repo::activity::display_workflow_error |
| bisect | graph | 5 | 3 | `src/repo/parallel_bisect/marks.rs:10` gitcraft::repo::graph_view::ROW_GAP |
| changes | app | 3 | 1 | `src/repo/detail/extras/notes_view.rs:243` gitcraft::manual::command |
| changes | conflicts | 3 | 1 | `src/repo/detail/commit.rs:16` gitcraft::repo::remerge::panel::Mode |
| changes | diff | 34 | 17 | `src/repo/checkout_blocked/carry.rs:13` gitcraft::repo::diff::literal_path |
| changes | graph | 6 | 2 | `src/repo/content_recovery/mod.rs:3` gitcraft::repo::saved_views::reader::Reader |
| changes | review | 11 | 6 | `src/repo/detail/changes.rs:44` gitcraft::repo::review_workflow::continuous::ReviewSource |
| changes | rewrite | 22 | 4 | `src/repo/detail/changes.rs:164` gitcraft::repo::stack::StackAct |
| changes | sidebar | 18 | 7 | `src/repo/detail/all_files.rs:11` gitcraft::repo::branch_tree |
| conflicts | agents | 36 | 13 | `src/repo/conflict/mod.rs:100` gitcraft::agent::handoff::Sides |
| conflicts | changes | 3 | 2 | `src/repo/conflict/view.rs:173` gitcraft::repo::unity::UnityAct |
| conflicts | diff | 4 | 1 | `src/repo/conflict/rerere/commands.rs:9` gitcraft::repo::diff::literal_path |
| conflicts | graph | 1 | 1 | `src/repo/resolver/graph.rs:154` gitcraft::repo::graph_view::row::paint::pill |
| conflicts | sync | 1 | 1 | `src/repo/core_seams/conflict_badges.rs:98` gitcraft::repo::pull::PullMode |
| diff | agents | 12 | 9 | `src/repo/diff/load.rs:231` gitcraft::authorship::Who |
| diff | doctor | 6 | 1 | `src/repo/file_comparison/view.rs:290` gitcraft::repo::activity::display_workflow_error |
| diff | graph | 7 | 1 | `src/repo/line_dossier/mod.rs:4` gitcraft::repo::saved_views::reader::Reader |
| diff | review | 24 | 13 | `src/repo/diff/link.rs:10` gitcraft::repo::forge_link::LinkMenu |
| doctor | graph | 2 | 1 | `src/repo/cleanup.rs:2` gitcraft::repo::history_reader::Reader |
| forge | review | 2 | 2 | `src/forge/mod.rs:267` gitcraft::repo::issue_work::naming::branch_name |
| git | agents | 8 | 5 | `src/git/drafts/turn.rs:172` gitcraft::agent::checkpoint::Turn |
| git | app | 14 | 5 | `src/git/file_comparison/index_transaction.rs:3` gitcraft::standalone::access |
| git | bisect | 3 | 3 | `src/git/bundles.rs:155` gitcraft::parallel_bisect::bundle::identity |
| git | guidance | 5 | 3 | `src/git/backend.rs:169` gitcraft::practice::is_repository |
| git | repo-core | 2 | 2 | `src/git/command/local_execution.rs:32` gitcraft::repo::hang_diagnostics::stage |
| git | sync | 17 | 8 | `src/git/drafts/deferred/settlement.rs:369` gitcraft::push_preflight::fix::accept_rewrite |
| git | worktrees | 1 | 1 | `src/git/machines.rs:259` gitcraft::teleport::Machine |
| graph | agents | 3 | 3 | `src/repo/graph_view/row/paint.rs:119` gitcraft::repo::authorship_view::Lens |
| graph | bisect | 5 | 4 | `src/repo/graph_view/header.rs:73` gitcraft::repo::test_lane::TEST_LANE_TIP |
| graph | changes | 2 | 1 | `src/repo/graph_view/menu.rs:17` gitcraft::repo::commit_actions::revert_action |
| graph | conflicts | 6 | 3 | `src/repo/graph_view/menu.rs:18` gitcraft::repo::conflict_check::Op |
| graph | doctor | 2 | 1 | `src/repo/history_activity/view.rs:129` gitcraft::repo::activity::display_workflow_error |
| graph | guidance | 6 | 2 | `src/repo/graph_view/pills.rs:19` gitcraft::repo::guide::GuideAct |
| graph | release | 1 | 1 | `src/repo/graph_view/menu/references.rs:216` gitcraft::repo::release::ReleaseAct |
| graph | review | 6 | 4 | `src/repo/graph_view/pills.rs:20` gitcraft::repo::ci_status |
| graph | rewrite | 34 | 9 | `src/repo/graph_view/drag.rs:10` gitcraft::repo::drag_drop::offers::DragBranch |
| graph | sidebar | 4 | 4 | `src/repo/graph_view/badge.rs:6` gitcraft::repo::sidebar |
| graph | terminal | 2 | 2 | `src/repo/graph_view/menu.rs:255` gitcraft::repo::custom_commands::menu_items |
| guidance | app | 3 | 2 | `src/repo/guide/hints.rs:50` gitcraft::manual::command |
| guidance | graph | 7 | 3 | `src/repo/guide/mod.rs:18` gitcraft::repo::branch_visibility::Change |
| insights | agents | 17 | 10 | `src/insights/conflict_radar/mod.rs:16` gitcraft::collision |
| insights | bisect | 1 | 1 | `src/repo/insights/coverage/mod.rs:110` gitcraft::repo::test_lane::LaneAct |
| insights | conflicts | 1 | 1 | `src/repo/insights/mod.rs:276` gitcraft::repo::resolver::ResolverAct |
| insights | diff | 17 | 5 | `src/repo/atlas/view.rs:436` gitcraft::repo::file_view::FileTab |
| insights | doctor | 1 | 1 | `src/repo/atlas/view.rs:125` gitcraft::repo::activity::display_workflow_error |
| insights | graph | 12 | 3 | `src/repo/atlas/byte_history.rs:2` gitcraft::repo::history_reader::Reader |
| insights | release | 6 | 1 | `src/repo/insights/constellation/picture.rs:16` gitcraft::repo::versions::VersionsAct |
| insights | review | 2 | 1 | `src/repo/insights/context.rs:96` gitcraft::repo::forge_selection::lifetime |
| insights | terminal | 1 | 1 | `src/repo/insights/constellation/picture.rs:313` gitcraft::repo::terminals::TerminalAct |
| os | agents | 27 | 6 | `src/instance/client.rs:55` gitcraft::mcp::wire::Reply |
| os | app | 4 | 3 | `src/instance/message.rs:232` gitcraft::launch::args::parse |
| os | remote | 7 | 6 | `src/instance/message.rs:127` gitcraft::remote::askpass::same |
| release | diff | 7 | 4 | `src/repo/versions/mod.rs:13` gitcraft::repo::diff |
| release | doctor | 1 | 1 | `src/repo/postcards/dialog.rs:186` gitcraft::repo::activity::display_workflow_error |
| release | graph | 4 | 3 | `src/repo/postcards/mod.rs:6` gitcraft::repo::history_reader::Reader |
| remote | repo-core | 1 | 1 | `src/folder_browser.rs:146` gitcraft::repo::widgets::primary_button |
| repo-core | app | 16 | 8 | `src/repo/core_seams/message_dispatch.rs:103` gitcraft::tray::watch::owns_fetch |
| review | agents | 17 | 11 | `src/repo/ci_view/page.rs:213` gitcraft::repo::agents::AgentAct |
| review | changes | 1 | 1 | `src/repo/pr_view/page.rs:240` gitcraft::repo::detail::files_header |
| review | diff | 29 | 8 | `src/repo/pr_review/lines.rs:7` gitcraft::repo::diff::Kind |
| review | release | 1 | 1 | `src/repo/pr_view/page.rs:68` gitcraft::repo::versions::pr_since::since_banner |
| review | rewrite | 6 | 4 | `src/repo/dco/mod.rs:337` gitcraft::repo::timeline::Awaiting |
| review | sidebar | 2 | 2 | `src/repo/gerrit.rs:22` gitcraft::repo::sidebar::remotes::remote_short |
| review | sync | 10 | 6 | `src/repo/review_workflow/echo.rs:179` gitcraft::push_preflight::Plan |
| review | terminal | 1 | 1 | `src/repo/review_workflow/choreography/rehearsal.rs:27` gitcraft::custom_commands::program::captured_program |
| rewrite | agents | 2 | 1 | `src/repo/ghost/mod.rs:295` gitcraft::repo::agent::preview::Preview |
| rewrite | app | 2 | 1 | `src/repo/timeline/navigation.rs:76` gitcraft::manual::command |
| rewrite | diff | 43 | 7 | `src/repo/surgery/absorb/mod.rs:20` gitcraft::repo::diff::Kind |
| rewrite | doctor | 4 | 2 | `src/repo/email/mod.rs:225` gitcraft::repo::activity::display_workflow_error |
| rewrite | graph | 4 | 2 | `src/repo/reflog/menu.rs:8` gitcraft::repo::graph_view::menu::rewrite::Rewrite |
| rewrite | sidebar | 9 | 5 | `src/repo/ghost/op.rs:134` gitcraft::repo::sidebar::remotes::remote_of |
| rewrite | sync | 7 | 1 | `src/repo/ghost/op.rs:4` gitcraft::repo::pull::PullMode |
| runtime | forge | 4 | 2 | `src/diagnostics.rs:32` gitcraft::forge::cli::ToolReport |
| runtime | git | 12 | 10 | `src/diagnostics.rs:12` gitcraft::git::doctor::config |
| runtime | remote | 7 | 4 | `src/diagnostics.rs:36` gitcraft::connections::inherit |
| runtime | update | 3 | 1 | `src/diagnostics.rs:88` gitcraft::update::CURRENT |
| settings | agents | 32 | 10 | `src/settings/agents.rs:31` gitcraft::mcp::program |
| settings | app | 12 | 7 | `src/settings/appearance_page.rs:51` gitcraft::app_buttons::AppButton |
| settings | changes | 4 | 1 | `src/drafts/keeper.rs:9` gitcraft::repo::drafts::DraftShape |
| settings | diff | 1 | 1 | `src/settings/preview/appearance.rs:21` gitcraft::repo::highlight::highlight |
| settings | graph | 30 | 11 | `src/settings/graph_page.rs:33` gitcraft::repo::history_filter::edit |
| settings | repo-core | 1 | 1 | `src/settings/preview/files.rs:104` gitcraft::repo::widgets::status_color |
| settings | sync | 15 | 2 | `src/settings/git_page.rs:56` gitcraft::repo::pull::PullMode |
| settings | terminal | 22 | 11 | `src/settings/custom_commands.rs:8` gitcraft::custom_commands |
| settings | update | 3 | 2 | `src/settings/general_page.rs:127` gitcraft::update::CURRENT |
| sidebar | agents | 1 | 1 | `src/repo/sidebar/worktrees.rs:53` gitcraft::repo::fence_view::list_violations |
| sidebar | app | 2 | 2 | `src/repo/sidebar/mod.rs:225` gitcraft::app::title_bar::FOLDED_STRIP_MOVES_WINDOW |
| sidebar | bisect | 1 | 1 | `src/repo/sidebar/branches/checkout.rs:132` gitcraft::repo::interaction::interaction_item |
| sidebar | changes | 4 | 2 | `src/repo/sidebar/submodules.rs:9` gitcraft::repo::keep_submodules::update_all |
| sidebar | conflicts | 8 | 2 | `src/repo/sidebar/branches.rs:15` gitcraft::repo::conflict_check::Op |
| sidebar | doctor | 5 | 1 | `src/repo/sidebar/branches/publication.rs:28` gitcraft::repo::repository_tools::Act |
| sidebar | graph | 21 | 5 | `src/repo/sidebar/branches.rs:14` gitcraft::repo::branch_visibility::local_ref |
| sidebar | release | 5 | 3 | `src/repo/sidebar/branches/menu.rs:36` gitcraft::repo::hotfix::HotfixAct |
| sidebar | review | 10 | 6 | `src/repo/sidebar/branches.rs:20` gitcraft::repo::ci_status |
| sidebar | rewrite | 31 | 4 | `src/repo/sidebar/branch_delete.rs:8` gitcraft::repo::ghost::offer |
| sidebar | terminal | 6 | 3 | `src/repo/sidebar/branches/menu.rs:172` gitcraft::repo::custom_commands::menu_items |
| sidebar | worktrees | 4 | 4 | `src/repo/sidebar/worktrees.rs:11` gitcraft::repo::worktree_tab |
| sync | doctor | 1 | 1 | `src/repo/ssh_setup/mod.rs:246` gitcraft::repo::activity::display_workflow_error |
| sync | graph | 2 | 1 | `src/repo/sparse.rs:2` gitcraft::repo::history_reader::Reader |
| sync | guidance | 1 | 1 | `src/repo/ssh_setup/worker.rs:27` gitcraft::practice::is_repository |
| sync | review | 5 | 2 | `src/push_preflight.rs:88` gitcraft::repo::forks::guard::Scope |
| sync | rewrite | 9 | 4 | `src/repo/pull.rs:85` gitcraft::repo::ghost::offer |
| sync | terminal | 8 | 2 | `src/repo/ssh_setup/mod.rs:13` gitcraft::terminal::Terminal |
| terminal | doctor | 1 | 1 | `src/repo/devcontainer/mod.rs:258` gitcraft::repo::activity::display_workflow_error |
| terminal | guidance | 3 | 3 | `src/terminal/mod.rs:111` gitcraft::practice::is_repository |
| ui-kit | app | 2 | 1 | `src/keymap.rs:167` gitcraft::manual::command |
| ui-kit | git | 1 | 1 | `src/theme/files/display.rs:70` gitcraft::git::errors::display_error |
| ui-kit | settings | 4 | 1 | `src/keymap.rs:144` gitcraft::settings::Settings |

### Cross-domain re-exports (9)

- `gitcraft::agent::sh` re-exports `gitcraft::remote::sh_quote` (remote) at `src/agent/sh.rs:3`
- `gitcraft::app::clone::fields` re-exports `gitcraft::a11y::singleline_enter` (ui-kit) at `src/app/clone/fields.rs:168`
- `gitcraft::app::lifecycle` re-exports `gitcraft::instance::Start` (os) at `src/app/lifecycle.rs:107`
- `gitcraft::repo::parallel_bisect` re-exports `gitcraft::repo::dialogs::parallel_bisect::ParallelForm` (repo-core) at `src/repo/parallel_bisect/mod.rs:23`
- `gitcraft::repo` re-exports `gitcraft::repo::drafts::DraftShape` (changes) at `src/repo/mod.rs:187`
- `gitcraft::repo` re-exports `gitcraft::repo::test_lane::TEST_LANE_TIP` (bisect) at `src/repo/mod.rs:192`
- `gitcraft::repo::state` re-exports `gitcraft::repo::commit_search::Search` (graph) at `src/repo/state.rs:3`
- `gitcraft::repo::widgets` re-exports `gitcraft::a11y::a11y` (ui-kit) at `src/repo/widgets/mod.rs:10`
- `gitcraft::repo::widgets` re-exports `gitcraft::a11y::a11y_selected` (ui-kit) at `src/repo/widgets/mod.rs:10`

<details><summary>All 291 domain edges</summary>

| From | To | Policy | References | Symbols |
| --- | --- | --- | ---: | ---: |
| agents | app | forbidden | 3 | 2 |
| agents | conflicts | forbidden | 20 | 6 |
| agents | diff | forbidden | 5 | 2 |
| agents | doctor | forbidden | 2 | 1 |
| agents | forge | layered | 40 | 15 |
| agents | git | layered | 158 | 33 |
| agents | graph | forbidden | 5 | 3 |
| agents | os | layered | 7 | 5 |
| agents | remote | layered | 118 | 27 |
| agents | repo-core | declared | 234 | 22 |
| agents | review | forbidden | 8 | 3 |
| agents | rewrite | forbidden | 27 | 11 |
| agents | runtime | layered | 448 | 9 |
| agents | settings | layered | 9 | 1 |
| agents | sidebar | forbidden | 8 | 4 |
| agents | terminal | forbidden | 12 | 4 |
| agents | ui-kit | layered | 146 | 13 |
| agents | update | forbidden | 1 | 1 |
| app | agents | layered | 17 | 7 |
| app | bisect | layered | 1 | 1 |
| app | changes | layered | 2 | 1 |
| app | conflicts | layered | 6 | 1 |
| app | diff | layered | 7 | 2 |
| app | doctor | layered | 5 | 2 |
| app | forge | layered | 63 | 22 |
| app | git | layered | 77 | 25 |
| app | guidance | layered | 7 | 3 |
| app | os | layered | 55 | 32 |
| app | remote | layered | 158 | 43 |
| app | repo-core | layered | 68 | 14 |
| app | runtime | layered | 710 | 41 |
| app | settings | layered | 114 | 35 |
| app | ui-kit | layered | 408 | 57 |
| app | update | layered | 33 | 15 |
| app | worktrees | layered | 167 | 58 |
| bisect | app | forbidden | 2 | 1 |
| bisect | doctor | forbidden | 3 | 1 |
| bisect | git | layered | 217 | 50 |
| bisect | graph | forbidden | 5 | 3 |
| bisect | os | layered | 3 | 1 |
| bisect | remote | layered | 66 | 23 |
| bisect | repo-core | declared | 201 | 14 |
| bisect | runtime | layered | 298 | 18 |
| bisect | settings | layered | 3 | 1 |
| bisect | ui-kit | layered | 56 | 8 |
| changes | app | forbidden | 3 | 1 |
| changes | conflicts | forbidden | 3 | 1 |
| changes | diff | forbidden | 34 | 17 |
| changes | forge | layered | 25 | 9 |
| changes | git | layered | 393 | 119 |
| changes | graph | forbidden | 6 | 2 |
| changes | remote | layered | 9 | 4 |
| changes | repo-core | declared | 554 | 38 |
| changes | review | forbidden | 11 | 6 |
| changes | rewrite | forbidden | 22 | 4 |
| changes | runtime | layered | 586 | 5 |
| changes | settings | layered | 33 | 8 |
| changes | sidebar | forbidden | 18 | 7 |
| changes | ui-kit | layered | 258 | 22 |
| conflicts | agents | forbidden | 36 | 13 |
| conflicts | changes | forbidden | 3 | 2 |
| conflicts | diff | forbidden | 4 | 1 |
| conflicts | git | layered | 300 | 69 |
| conflicts | graph | forbidden | 1 | 1 |
| conflicts | remote | layered | 42 | 14 |
| conflicts | repo-core | declared | 268 | 19 |
| conflicts | runtime | layered | 365 | 10 |
| conflicts | settings | layered | 7 | 1 |
| conflicts | sync | forbidden | 1 | 1 |
| conflicts | ui-kit | layered | 182 | 14 |
| diff | agents | forbidden | 12 | 9 |
| diff | doctor | forbidden | 6 | 1 |
| diff | forge | layered | 47 | 12 |
| diff | git | layered | 388 | 78 |
| diff | graph | forbidden | 7 | 1 |
| diff | remote | layered | 21 | 14 |
| diff | repo-core | declared | 493 | 31 |
| diff | review | forbidden | 24 | 13 |
| diff | runtime | layered | 1003 | 22 |
| diff | settings | layered | 17 | 1 |
| diff | ui-kit | layered | 366 | 32 |
| doctor | forge | layered | 1 | 1 |
| doctor | git | layered | 497 | 196 |
| doctor | graph | forbidden | 2 | 1 |
| doctor | remote | layered | 31 | 8 |
| doctor | repo-core | declared | 339 | 20 |
| doctor | runtime | layered | 694 | 7 |
| doctor | settings | layered | 15 | 2 |
| doctor | ui-kit | layered | 131 | 12 |
| forge | git | peer | 100 | 24 |
| forge | remote | peer | 36 | 8 |
| forge | review | forbidden | 2 | 2 |
| forge | runtime | layered | 439 | 18 |
| forge | settings | peer | 1 | 1 |
| git | agents | forbidden | 8 | 5 |
| git | app | forbidden | 14 | 5 |
| git | bisect | forbidden | 3 | 3 |
| git | forge | peer | 4 | 2 |
| git | guidance | forbidden | 5 | 3 |
| git | os | layered | 2 | 2 |
| git | remote | peer | 542 | 50 |
| git | repo-core | forbidden | 2 | 2 |
| git | runtime | layered | 2270 | 32 |
| git | settings | peer | 10 | 3 |
| git | sync | forbidden | 17 | 8 |
| git | worktrees | forbidden | 1 | 1 |
| graph | agents | forbidden | 3 | 3 |
| graph | bisect | forbidden | 5 | 4 |
| graph | changes | forbidden | 2 | 1 |
| graph | conflicts | forbidden | 6 | 3 |
| graph | doctor | forbidden | 2 | 1 |
| graph | forge | layered | 5 | 3 |
| graph | git | layered | 199 | 50 |
| graph | guidance | forbidden | 6 | 2 |
| graph | release | forbidden | 1 | 1 |
| graph | remote | layered | 7 | 4 |
| graph | repo-core | declared | 230 | 25 |
| graph | review | forbidden | 6 | 4 |
| graph | rewrite | forbidden | 34 | 9 |
| graph | runtime | layered | 354 | 13 |
| graph | settings | layered | 39 | 6 |
| graph | sidebar | forbidden | 4 | 4 |
| graph | terminal | forbidden | 2 | 2 |
| graph | ui-kit | layered | 192 | 34 |
| guidance | app | forbidden | 3 | 2 |
| guidance | git | layered | 17 | 9 |
| guidance | graph | forbidden | 7 | 3 |
| guidance | remote | layered | 2 | 1 |
| guidance | repo-core | declared | 74 | 16 |
| guidance | runtime | layered | 110 | 9 |
| guidance | settings | layered | 13 | 1 |
| guidance | ui-kit | layered | 25 | 10 |
| insights | agents | forbidden | 17 | 10 |
| insights | bisect | forbidden | 1 | 1 |
| insights | conflicts | forbidden | 1 | 1 |
| insights | diff | forbidden | 17 | 5 |
| insights | doctor | forbidden | 1 | 1 |
| insights | forge | layered | 257 | 36 |
| insights | git | layered | 213 | 49 |
| insights | graph | forbidden | 12 | 3 |
| insights | release | forbidden | 6 | 1 |
| insights | remote | layered | 15 | 9 |
| insights | repo-core | declared | 361 | 13 |
| insights | review | forbidden | 2 | 1 |
| insights | runtime | layered | 74 | 4 |
| insights | terminal | forbidden | 1 | 1 |
| insights | ui-kit | layered | 178 | 14 |
| os | agents | forbidden | 27 | 6 |
| os | app | forbidden | 4 | 3 |
| os | remote | forbidden | 7 | 6 |
| os | runtime | peer | 11 | 2 |
| release | diff | forbidden | 7 | 4 |
| release | doctor | forbidden | 1 | 1 |
| release | forge | layered | 38 | 8 |
| release | git | layered | 176 | 41 |
| release | graph | forbidden | 4 | 3 |
| release | remote | layered | 14 | 7 |
| release | repo-core | declared | 162 | 16 |
| release | runtime | layered | 262 | 4 |
| release | ui-kit | layered | 57 | 9 |
| remote | forge | peer | 11 | 7 |
| remote | git | peer | 16 | 9 |
| remote | os | layered | 32 | 13 |
| remote | repo-core | forbidden | 1 | 1 |
| remote | runtime | layered | 99 | 23 |
| remote | ui-kit | layered | 12 | 7 |
| repo-core | agents | layered | 23 | 13 |
| repo-core | app | forbidden | 16 | 8 |
| repo-core | bisect | layered | 124 | 34 |
| repo-core | changes | layered | 100 | 47 |
| repo-core | conflicts | layered | 40 | 22 |
| repo-core | diff | layered | 130 | 45 |
| repo-core | doctor | layered | 22 | 15 |
| repo-core | forge | layered | 58 | 26 |
| repo-core | git | layered | 442 | 136 |
| repo-core | graph | layered | 36 | 23 |
| repo-core | guidance | layered | 10 | 9 |
| repo-core | insights | layered | 12 | 5 |
| repo-core | release | layered | 21 | 10 |
| repo-core | remote | layered | 37 | 14 |
| repo-core | review | layered | 76 | 45 |
| repo-core | rewrite | layered | 48 | 30 |
| repo-core | runtime | layered | 797 | 22 |
| repo-core | settings | layered | 31 | 3 |
| repo-core | sidebar | layered | 26 | 10 |
| repo-core | sync | layered | 47 | 26 |
| repo-core | terminal | layered | 46 | 18 |
| repo-core | ui-kit | layered | 508 | 45 |
| repo-core | update | layered | 1 | 1 |
| repo-core | worktrees | layered | 2 | 1 |
| review | agents | forbidden | 17 | 11 |
| review | changes | forbidden | 1 | 1 |
| review | diff | forbidden | 29 | 8 |
| review | forge | layered | 458 | 85 |
| review | git | layered | 249 | 46 |
| review | release | forbidden | 1 | 1 |
| review | remote | layered | 28 | 10 |
| review | repo-core | declared | 569 | 43 |
| review | rewrite | forbidden | 6 | 4 |
| review | runtime | layered | 624 | 18 |
| review | settings | layered | 9 | 1 |
| review | sidebar | forbidden | 2 | 2 |
| review | sync | forbidden | 10 | 6 |
| review | terminal | forbidden | 1 | 1 |
| review | ui-kit | layered | 177 | 15 |
| rewrite | agents | forbidden | 2 | 1 |
| rewrite | app | forbidden | 2 | 1 |
| rewrite | diff | forbidden | 43 | 7 |
| rewrite | doctor | forbidden | 4 | 2 |
| rewrite | forge | layered | 28 | 10 |
| rewrite | git | layered | 461 | 107 |
| rewrite | graph | forbidden | 4 | 2 |
| rewrite | remote | layered | 32 | 15 |
| rewrite | repo-core | declared | 840 | 40 |
| rewrite | runtime | layered | 776 | 10 |
| rewrite | settings | layered | 2 | 1 |
| rewrite | sidebar | forbidden | 9 | 5 |
| rewrite | sync | forbidden | 7 | 1 |
| rewrite | ui-kit | layered | 287 | 26 |
| runtime | forge | forbidden | 4 | 2 |
| runtime | git | forbidden | 12 | 10 |
| runtime | os | peer | 6 | 2 |
| runtime | remote | forbidden | 7 | 4 |
| runtime | update | forbidden | 3 | 1 |
| settings | agents | forbidden | 32 | 10 |
| settings | app | forbidden | 12 | 7 |
| settings | changes | forbidden | 4 | 1 |
| settings | diff | forbidden | 1 | 1 |
| settings | forge | peer | 8 | 5 |
| settings | git | peer | 75 | 29 |
| settings | graph | forbidden | 30 | 11 |
| settings | os | layered | 12 | 12 |
| settings | remote | peer | 39 | 24 |
| settings | repo-core | forbidden | 1 | 1 |
| settings | runtime | layered | 551 | 23 |
| settings | sync | forbidden | 15 | 2 |
| settings | terminal | forbidden | 22 | 11 |
| settings | ui-kit | layered | 234 | 40 |
| settings | update | forbidden | 3 | 2 |
| sidebar | agents | forbidden | 1 | 1 |
| sidebar | app | forbidden | 2 | 2 |
| sidebar | bisect | forbidden | 1 | 1 |
| sidebar | changes | forbidden | 4 | 2 |
| sidebar | conflicts | forbidden | 8 | 2 |
| sidebar | doctor | forbidden | 5 | 1 |
| sidebar | forge | layered | 23 | 6 |
| sidebar | git | layered | 89 | 17 |
| sidebar | graph | forbidden | 21 | 5 |
| sidebar | release | forbidden | 5 | 3 |
| sidebar | remote | layered | 15 | 7 |
| sidebar | repo-core | declared | 343 | 29 |
| sidebar | review | forbidden | 10 | 6 |
| sidebar | rewrite | forbidden | 31 | 4 |
| sidebar | runtime | layered | 262 | 3 |
| sidebar | settings | layered | 12 | 1 |
| sidebar | terminal | forbidden | 6 | 3 |
| sidebar | ui-kit | layered | 224 | 32 |
| sidebar | worktrees | forbidden | 4 | 4 |
| sync | doctor | forbidden | 1 | 1 |
| sync | forge | layered | 25 | 7 |
| sync | git | layered | 74 | 34 |
| sync | graph | forbidden | 2 | 1 |
| sync | guidance | forbidden | 1 | 1 |
| sync | remote | layered | 83 | 19 |
| sync | repo-core | declared | 183 | 23 |
| sync | review | forbidden | 5 | 2 |
| sync | rewrite | forbidden | 9 | 4 |
| sync | runtime | layered | 347 | 13 |
| sync | settings | layered | 25 | 3 |
| sync | terminal | forbidden | 8 | 2 |
| sync | ui-kit | layered | 30 | 10 |
| terminal | doctor | forbidden | 1 | 1 |
| terminal | forge | layered | 1 | 1 |
| terminal | git | layered | 5 | 4 |
| terminal | guidance | forbidden | 3 | 3 |
| terminal | remote | layered | 19 | 11 |
| terminal | repo-core | declared | 73 | 12 |
| terminal | runtime | layered | 139 | 12 |
| terminal | settings | layered | 6 | 1 |
| terminal | ui-kit | layered | 35 | 17 |
| ui-kit | app | forbidden | 2 | 1 |
| ui-kit | git | forbidden | 1 | 1 |
| ui-kit | runtime | layered | 94 | 6 |
| ui-kit | settings | forbidden | 4 | 1 |
| update | os | layered | 1 | 1 |
| update | runtime | layered | 22 | 4 |
| worktrees | git | layered | 95 | 20 |
| worktrees | remote | layered | 85 | 24 |
| worktrees | repo-core | declared | 10 | 1 |
| worktrees | runtime | layered | 97 | 2 |
| worktrees | ui-kit | layered | 15 | 9 |

</details>

## `RepoTab` state (246 fields, `src/repo/tab_owner/model.rs`)

A field count is not a finding; mutation by a domain that does not own the field is.

### Field hotspots

| Field | Owner | Domains mutating | Domains reading |
| --- | --- | ---: | ---: |
| `status` | repo-core | 16 | 7 |
| `busy_op` | repo-core | 12 | 16 |
| `dialog` | repo-core | 11 | 9 |
| `diff` | repo-core | 11 | 5 |
| `file_view` | diff | 11 | 3 |
| `ci_view` | review | 10 | 4 |
| `finished` | repo-core | 9 | 0 |
| `op_started` | repo-core | 8 | 2 |
| `scroll_to` | repo-core | 8 | 2 |
| `tour` | guidance | 7 | 2 |
| `last_op` | repo-core | 6 | 5 |
| `doctor` | doctor | 6 | 2 |
| `conflict` | conflicts | 5 | 12 |
| `compare` | repo-core | 5 | 6 |
| `bakeoff` | agents | 5 | 2 |
| `selected` | repo-core | 4 | 8 |
| `prs` | repo-core | 4 | 7 |
| `stash_view` | repo-core | 4 | 6 |
| `commit_body` | repo-core | 4 | 4 |
| `commit_summary` | repo-core | 4 | 4 |
| `prs_loaded` | repo-core | 4 | 4 |
| `search` | graph | 4 | 4 |
| `versions` | release | 4 | 4 |
| `terminals` | terminal | 4 | 3 |
| `dossier` | diff | 4 | 2 |

### Domains by tab state touched

| Domain | Fields read | Fields mutated | Foreign fields mutated |
| --- | ---: | ---: | ---: |
| repo-core | 145 | 158 | 51 |
| review | 49 | 52 | 42 |
| changes | 53 | 40 | 27 |
| rewrite | 44 | 38 | 26 |
| graph | 54 | 35 | 20 |
| diff | 35 | 26 | 20 |
| doctor | 26 | 19 | 14 |
| sync | 21 | 17 | 13 |
| insights | 22 | 16 | 13 |
| sidebar | 26 | 13 | 12 |
| agents | 23 | 19 | 11 |
| release | 25 | 13 | 8 |
| conflicts | 25 | 15 | 7 |
| bisect | 18 | 12 | 7 |
| guidance | 23 | 10 | 7 |
| worktrees | 18 | 7 | 7 |
| terminal | 8 | 7 | 4 |
| app | 1 | 0 | 0 |

### Cross-feature mutations (238)

| Domain | Field | Owner | Sites | First site |
| --- | --- | --- | ---: | --- |
| agents | `busy_op` | repo-core | 1 | `src/repo/fence_view/mod.rs:265` Write |
| agents | `ci_view` | review | 1 | `src/repo/bakeoff/mod.rs:175` Write |
| agents | `commit_body` | repo-core | 2 | `src/repo/ai/message.rs:240` Write |
| agents | `commit_summary` | repo-core | 2 | `src/repo/ai/message.rs:239` Write |
| agents | `diff` | repo-core | 1 | `src/repo/bakeoff/mod.rs:176` Write |
| agents | `file_view` | diff | 1 | `src/repo/bakeoff/mod.rs:177` Write |
| agents | `open_request` | repo-core | 1 | `src/repo/agents/mod.rs:121` Write |
| agents | `open_when_done` | repo-core | 1 | `src/repo/agents/mod.rs:351` Write |
| agents | `search` | graph | 12 | `src/repo/authorship_view.rs:179` MutCall |
| agents | `status` | repo-core | 8 | `src/repo/agent/mod.rs:418` Write |
| agents | `terminals` | terminal | 2 | `src/repo/agents/handoff.rs:184` MutCall |
| bisect | `bakeoff` | agents | 1 | `src/repo/local_ci/actions.rs:34` Write |
| bisect | `busy_op` | repo-core | 5 | `src/repo/bisect/mod.rs:193` Write |
| bisect | `dialog` | repo-core | 2 | `src/repo/interaction/mod.rs:82` Write |
| bisect | `doctor` | doctor | 1 | `src/repo/local_ci/actions.rs:33` Write |
| bisect | `finished` | repo-core | 4 | `src/repo/bisect/mod.rs:316` MutCall |
| bisect | `op_started` | repo-core | 5 | `src/repo/bisect/mod.rs:194` Write |
| bisect | `status` | repo-core | 14 | `src/repo/bisect/mod.rs:153` Write |
| changes | `amend` | repo-core | 1 | `src/repo/detail/commit_box.rs:79` Write |
| changes | `busy_op` | repo-core | 5 | `src/repo/content_recovery/mod.rs:116` Write |
| changes | `commit_body` | repo-core | 4 | `src/repo/detail/compose/mod.rs:198` Write |
| changes | `commit_summary` | repo-core | 5 | `src/repo/detail/compose/mod.rs:197` Write |
| changes | `compare` | repo-core | 1 | `src/repo/commit_changes.rs:119` Write |
| changes | `conflict` | conflicts | 2 | `src/repo/unity/merge.rs:15` MutCall |
| changes | `dco` | review | 1 | `src/repo/detail/commit_box.rs:58` MutCall |
| changes | `detail_error` | repo-core | 1 | `src/repo/detail_cache.rs:114` Write |
| changes | `dialog` | repo-core | 11 | `src/repo/checkout_blocked.rs:191` Write |
| changes | `diff` | repo-core | 5 | `src/repo/commit_changes.rs:95` Write |
| changes | `doctor` | doctor | 2 | `src/repo/commit_changes.rs:60` Write |
| changes | `drafts_loaded` | repo-core | 1 | `src/repo/drafts.rs:116` Write |
| changes | `file_view` | diff | 1 | `src/repo/commit_changes.rs:118` Write |
| changes | `focus_commit_summary` | repo-core | 1 | `src/repo/detail/commit_box.rs:116` MutBorrow |
| changes | `focus_wip` | repo-core | 1 | `src/repo/revert_piece.rs:169` Write |
| changes | `guide` | guidance | 1 | `src/repo/detail/commit_box.rs:120` MutCall |
| changes | `last_op` | repo-core | 1 | `src/repo/partial_stash/mod.rs:211` Write |
| changes | `nav_jump` | repo-core | 1 | `src/repo/commit_changes.rs:115` Write |
| changes | `op_started` | repo-core | 5 | `src/repo/content_recovery/mod.rs:117` Write |
| changes | `pending_goto` | repo-core | 1 | `src/repo/commit_changes.rs:116` Write |
| changes | `pr_drafts` | repo-core | 1 | `src/repo/drafts.rs:124` MutCall |
| changes | `prompt_session` | repo-core | 2 | `src/repo/detail/commit.rs:47` MutBorrow |
| changes | `queued` | repo-core | 1 | `src/repo/detached/mod.rs:269` MutCall |
| changes | `rebase` | rewrite | 1 | `src/repo/drafts.rs:143` Write |
| changes | `reword` | repo-core | 2 | `src/repo/detail/commit.rs:115` MutCall |
| changes | `status` | repo-core | 19 | `src/repo/commit_changes.rs:107` Write |
| changes | `update_submodules` | repo-core | 2 | `src/repo/keep_submodules.rs:36` Write |
| conflicts | `busy_op` | repo-core | 8 | `src/repo/resolver/apply.rs:43` Write |
| conflicts | `dialog` | repo-core | 1 | `src/repo/rehearse/mod.rs:131` Write |
| conflicts | `finished` | repo-core | 2 | `src/repo/rehearse/mod.rs:256` MutCall |
| conflicts | `parked` | repo-core | 1 | `src/repo/resolver/mod.rs:205` MutCall |
| conflicts | `queued` | repo-core | 1 | `src/repo/resolver/apply.rs:125` MutCall |
| conflicts | `scroll_to` | repo-core | 1 | `src/repo/resolver/dispatch.rs:138` Write |
| conflicts | `status` | repo-core | 34 | `src/repo/conflict/decisions/mod.rs:172` Write |
| diff | `agents` | agents | 1 | `src/repo/diff/view.rs:154` MutBorrow |
| diff | `busy_op` | repo-core | 3 | `src/repo/file_comparison/selection.rs:142` Write |
| diff | `ci_view` | review | 2 | `src/repo/diff_split.rs:116` Write |
| diff | `conflict` | conflicts | 1 | `src/repo/file_view/line_history.rs:32` Write |
| diff | `diff` | repo-core | 19 | `src/repo/diff/algorithm.rs:113` MutBorrow |
| diff | `diff_as_file` | repo-core | 5 | `src/repo/diff/encrypted/mod.rs:307` Write |
| diff | `diff_rendered` | repo-core | 5 | `src/repo/diff/encrypted/mod.rs:308` Write |
| diff | `diff_rich_for` | repo-core | 1 | `src/repo/diff/rich/tab.rs:63` Write |
| diff | `insights` | insights | 1 | `src/repo/diff_split.rs:111` Write |
| diff | `last_op` | repo-core | 1 | `src/repo/file_comparison/selection.rs:144` Write |
| diff | `op_started` | repo-core | 3 | `src/repo/file_comparison/selection.rs:143` Write |
| diff | `pr_drafts` | repo-core | 1 | `src/repo/diff/view.rs:158` MutCall |
| diff | `pr_view` | review | 1 | `src/repo/diff/view.rs:156` MutBorrow |
| diff | `scroll_to` | repo-core | 1 | `src/repo/diff_split.rs:59` Write |
| diff | `search` | graph | 2 | `src/repo/find/keys.rs:69` Write |
| diff | `status` | repo-core | 2 | `src/repo/file_comparison/mod.rs:131` Write |
| diff | `structural_pending` | repo-core | 3 | `src/repo/diff/structural/mod.rs:79` MutBorrow |
| diff | `structural_running` | repo-core | 2 | `src/repo/diff/structural/mod.rs:78` Write |
| diff | `tour` | guidance | 2 | `src/repo/diff_split.rs:109` Write |
| diff | `versions` | release | 1 | `src/repo/diff_split.rs:110` Write |
| doctor | `busy_op` | repo-core | 4 | `src/repo/cleanup.rs:220` Write |
| doctor | `ci_view` | review | 1 | `src/repo/doctor/mod.rs:191` Write |
| doctor | `dialog` | repo-core | 12 | `src/repo/repository_tools/actions.rs:47` Write |
| doctor | `diff` | repo-core | 1 | `src/repo/doctor/mod.rs:189` Write |
| doctor | `file_view` | diff | 1 | `src/repo/doctor/mod.rs:190` Write |
| doctor | `insights` | insights | 1 | `src/repo/doctor/mod.rs:194` Write |
| doctor | `last_op` | repo-core | 3 | `src/repo/repository_tools/dependency_jobs.rs:33` Write |
| doctor | `op_started` | repo-core | 3 | `src/repo/repository_tools/dependency_jobs.rs:32` Write |
| doctor | `outbox_last` | repo-core | 2 | `src/repo/repository_tools/context.rs:9` Write |
| doctor | `outbox_reading` | repo-core | 2 | `src/repo/repository_tools/context.rs:8` Write |
| doctor | `preflight_selection` | repo-core | 2 | `src/repo/repository_tools/push_continuation.rs:80` Write |
| doctor | `stacks` | rewrite | 1 | `src/repo/repository_tools/deferred.rs:121` Write |
| doctor | `status` | repo-core | 12 | `src/repo/doctor/mod.rs:183` Write |
| doctor | `tour` | guidance | 1 | `src/repo/doctor/mod.rs:193` Write |
| graph | `authorship` | agents | 3 | `src/repo/commit_search.rs:205` Write |
| graph | `ci_view` | review | 1 | `src/repo/history_activity/mod.rs:140` Write |
| graph | `commits` | repo-core | 2 | `src/repo/graph_view/mod.rs:157` MutBorrow |
| graph | `diff` | repo-core | 1 | `src/repo/history_activity/mod.rs:138` Write |
| graph | `file_view` | diff | 1 | `src/repo/history_activity/mod.rs:139` Write |
| graph | `ghost` | rewrite | 4 | `src/repo/graph_view/mod.rs:156` MutCall |
| graph | `graph` | repo-core | 4 | `src/repo/branch_bands.rs:177` Write |
| graph | `graph_labels` | repo-core | 2 | `src/repo/graph_view/readability.rs:21` MutCall |
| graph | `history` | repo-core | 1 | `src/repo/snapshot.rs:73` Write |
| graph | `index` | repo-core | 2 | `src/repo/graph_view/mod.rs:157` MutBorrow |
| graph | `log_from` | repo-core | 1 | `src/repo/branch_visibility.rs:238` Write |
| graph | `log_order` | repo-core | 1 | `src/repo/graph_order.rs:90` Write |
| graph | `refs` | repo-core | 1 | `src/repo/snapshot.rs:72` Write |
| graph | `reload_again` | repo-core | 2 | `src/repo/branch_visibility.rs:235` Write |
| graph | `scroll_to` | repo-core | 4 | `src/repo/core_seams/graph_narration.rs:225` Write |
| graph | `selected` | repo-core | 3 | `src/repo/graph_view/mod.rs:162` Write |
| graph | `snapshot_kept` | repo-core | 1 | `src/repo/snapshot.rs:87` Write |
| graph | `snapshot_shown` | repo-core | 1 | `src/repo/snapshot.rs:74` Write |
| graph | `stash_view` | repo-core | 1 | `src/repo/history_activity/mod.rs:228` Write |
| graph | `status` | repo-core | 4 | `src/repo/branch_bands.rs:181` Write |
| guidance | `ci_view` | review | 2 | `src/repo/guide/mod.rs:132` Write |
| guidance | `compare` | repo-core | 1 | `src/repo/guide/mod.rs:133` Write |
| guidance | `diff` | repo-core | 1 | `src/repo/tour/mod.rs:43` Write |
| guidance | `file_view` | diff | 1 | `src/repo/tour/mod.rs:44` Write |
| guidance | `practice_checking` | repo-core | 1 | `src/repo/practice.rs:77` Write |
| guidance | `scroll_to` | repo-core | 1 | `src/repo/guide/mod.rs:135` Write |
| guidance | `status` | repo-core | 4 | `src/repo/guide/mod.rs:117` Write |
| insights | `bakeoff` | agents | 1 | `src/repo/insights/mod.rs:291` Write |
| insights | `ci_view` | review | 1 | `src/repo/insights/mod.rs:286` Write |
| insights | `diff` | repo-core | 2 | `src/repo/insights/mod.rs:284` Write |
| insights | `doctor` | doctor | 1 | `src/repo/insights/mod.rs:290` Write |
| insights | `dossier` | diff | 1 | `src/repo/insights/mod.rs:282` MutCall |
| insights | `file_comparison` | diff | 1 | `src/repo/insights/mod.rs:279` Write |
| insights | `file_view` | diff | 1 | `src/repo/insights/mod.rs:285` Write |
| insights | `review_document` | review | 1 | `src/repo/insights/mod.rs:283` Write |
| insights | `status` | repo-core | 1 | `src/repo/insights/mod.rs:269` Write |
| insights | `symbols` | diff | 1 | `src/repo/insights/mod.rs:281` MutCall |
| insights | `timeline_view` | rewrite | 1 | `src/repo/insights/mod.rs:289` Write |
| insights | `tour` | guidance | 1 | `src/repo/insights/mod.rs:280` Write |
| insights | `versions` | release | 1 | `src/repo/insights/mod.rs:288` Write |
| release | `busy_op` | repo-core | 3 | `src/repo/release/mod.rs:251` Write |
| release | `ci_view` | review | 1 | `src/repo/versions/mod.rs:134` Write |
| release | `dialog` | repo-core | 1 | `src/repo/hotfix/mod.rs:70` Write |
| release | `diff` | repo-core | 1 | `src/repo/versions/mod.rs:132` Write |
| release | `file_view` | diff | 1 | `src/repo/versions/mod.rs:133` Write |
| release | `finished` | repo-core | 1 | `src/repo/release/mod.rs:401` MutCall |
| release | `op_started` | repo-core | 2 | `src/repo/release/mod.rs:252` Write |
| release | `status` | repo-core | 10 | `src/repo/hotfix/mod.rs:60` Write |
| review | `avatars_hinted` | repo-core | 1 | `src/repo/forge_selection.rs:282` Write |
| review | `bakeoff` | agents | 1 | `src/repo/pr_view/select.rs:36` Write |
| review | `branch_health` | sidebar | 1 | `src/repo/forge_selection.rs:277` Write |
| review | `busy_op` | repo-core | 1 | `src/repo/forks/completion.rs:99` Write |
| review | `commits` | repo-core | 1 | `src/repo/pr_view/select.rs:174` MutCall |
| review | `compare` | repo-core | 4 | `src/repo/forge_selection.rs:271` Write |
| review | `compare_requests` | repo-core | 1 | `src/repo/pr_view/select.rs:75` Write |
| review | `dialog` | repo-core | 5 | `src/repo/forge_selection.rs:284` Write |
| review | `diff` | repo-core | 5 | `src/repo/ci_view/mod.rs:137` Write |
| review | `doctor` | doctor | 1 | `src/repo/pr_view/select.rs:35` Write |
| review | `dossier` | diff | 2 | `src/repo/review_workflow/continuous.rs:70` Write |
| review | `file_comparison` | diff | 2 | `src/repo/review_workflow/continuous.rs:68` Write |
| review | `file_view` | diff | 4 | `src/repo/ci_view/mod.rs:138` Write |
| review | `finished` | repo-core | 1 | `src/repo/pr_review/send.rs:193` MutCall |
| review | `forge_generation` | repo-core | 1 | `src/repo/forge_selection.rs:230` Write |
| review | `forge_repository_generation` | repo-core | 1 | `src/repo/forge_selection.rs:232` Write |
| review | `hosting` | repo-core | 1 | `src/repo/forge_selection.rs:234` Write |
| review | `hotfix` | release | 1 | `src/repo/forge_selection.rs:267` Write |
| review | `last_op` | repo-core | 6 | `src/repo/forks/completion.rs:168` Write |
| review | `nav_jump` | repo-core | 1 | `src/repo/pr_view/select.rs:39` Write |
| review | `op_started` | repo-core | 1 | `src/repo/forks/completion.rs:100` Write |
| review | `open_when_done` | repo-core | 1 | `src/repo/issue_work/mod.rs:300` Write |
| review | `pending_goto` | repo-core | 1 | `src/repo/pr_view/select.rs:40` Write |
| review | `pending_prs` | repo-core | 1 | `src/repo/forge_selection.rs:245` Write |
| review | `pr_drafts` | repo-core | 12 | `src/repo/forge_selection.rs:252` MutCall |
| review | `pr_error` | repo-core | 1 | `src/repo/forge_selection.rs:246` Write |
| review | `pr_graph_head` | repo-core | 3 | `src/repo/forge_selection.rs:262` Write |
| review | `pr_requests` | repo-core | 1 | `src/repo/pr_view/mod.rs:133` Write |
| review | `pr_selection_request` | repo-core | 3 | `src/repo/forge_selection.rs:269` MutCall |
| review | `preflight_after` | repo-core | 1 | `src/repo/forks/completion.rs:220` Write |
| review | `prs` | repo-core | 1 | `src/repo/forge_selection.rs:243` MutCall |
| review | `prs_loaded` | repo-core | 1 | `src/repo/forge_selection.rs:247` Write |
| review | `reload_again` | repo-core | 1 | `src/repo/forge_selection.rs:186` Write |
| review | `scroll_to` | repo-core | 1 | `src/repo/pr_view/select.rs:115` Write |
| review | `selected` | repo-core | 2 | `src/repo/pr_view/select.rs:114` Write |
| review | `stacks` | rewrite | 1 | `src/repo/forge_selection.rs:281` Write |
| review | `stash_view` | repo-core | 1 | `src/repo/pr_view/select.rs:117` Write |
| review | `status` | repo-core | 34 | `src/repo/ci_view/mod.rs:131` Write |
| review | `symbols` | diff | 2 | `src/repo/review_workflow/continuous.rs:69` Write |
| review | `timeline` | rewrite | 1 | `src/repo/dco/mod.rs:336` MutBorrow |
| review | `tour` | guidance | 2 | `src/repo/ci_view/mod.rs:139` Write |
| review | `versions` | release | 4 | `src/repo/forge_selection.rs:263` MutCall |
| rewrite | `amend` | repo-core | 1 | `src/repo/fold/mod.rs:282` Write |
| rewrite | `busy_op` | repo-core | 13 | `src/repo/bundles.rs:134` Write |
| rewrite | `ci_view` | review | 3 | `src/repo/reflog/mod.rs:178` Write |
| rewrite | `commit_body` | repo-core | 1 | `src/repo/fold/mod.rs:281` MutCall |
| rewrite | `commit_summary` | repo-core | 1 | `src/repo/fold/mod.rs:280` MutCall |
| rewrite | `compare` | repo-core | 1 | `src/repo/reflog/mod.rs:395` Write |
| rewrite | `conflict` | conflicts | 1 | `src/repo/rewrite.rs:214` Write |
| rewrite | `detail` | repo-core | 1 | `src/repo/reflog/mod.rs:223` Write |
| rewrite | `detail_hash` | repo-core | 1 | `src/repo/reflog/mod.rs:224` Write |
| rewrite | `dialog` | repo-core | 5 | `src/repo/bundles.rs:12` Write |
| rewrite | `diff` | repo-core | 4 | `src/repo/reflog/mod.rs:180` Write |
| rewrite | `file_view` | diff | 4 | `src/repo/reflog/mod.rs:179` Write |
| rewrite | `finished` | repo-core | 2 | `src/repo/stack/publish.rs:181` MutCall |
| rewrite | `focus_wip` | repo-core | 1 | `src/repo/stack/restack.rs:183` Write |
| rewrite | `last_op` | repo-core | 3 | `src/repo/bundles.rs:138` Write |
| rewrite | `op_started` | repo-core | 9 | `src/repo/bundles.rs:135` Write |
| rewrite | `plain_rebase_request` | repo-core | 1 | `src/repo/rewrite.rs:234` Write |
| rewrite | `prs_loaded` | repo-core | 1 | `src/repo/stack/publish.rs:187` Write |
| rewrite | `queued` | repo-core | 1 | `src/repo/timeline/navigation.rs:98` MutCall |
| rewrite | `reword` | repo-core | 3 | `src/repo/quick_edit.rs:260` Write |
| rewrite | `scroll_to` | repo-core | 2 | `src/repo/reflog/mod.rs:220` Write |
| rewrite | `selected` | repo-core | 1 | `src/repo/reflog/mod.rs:404` Write |
| rewrite | `stash_view` | repo-core | 1 | `src/repo/reflog/mod.rs:397` Write |
| rewrite | `status` | repo-core | 76 | `src/repo/bundles.rs:69` Write |
| rewrite | `timeline_error` | repo-core | 1 | `src/repo/timeline/tab.rs:130` Write |
| rewrite | `tour` | guidance | 1 | `src/repo/timeline/view.rs:88` Write |
| sidebar | `branch_filter` | repo-core | 1 | `src/repo/sidebar/mod.rs:48` MutBorrow |
| sidebar | `dialog` | repo-core | 1 | `src/repo/branch_cleanup.rs:92` Write |
| sidebar | `focus_branch_filter` | repo-core | 1 | `src/repo/sidebar/mod.rs:53` MutBorrow |
| sidebar | `open_request` | repo-core | 1 | `src/repo/sidebar/pull_requests/worktrees.rs:96` Write |
| sidebar | `open_when_done` | repo-core | 1 | `src/repo/sidebar/pull_requests/worktrees.rs:106` Write |
| sidebar | `pending_prs` | repo-core | 3 | `src/repo/sidebar/pull_requests.rs:154` Write |
| sidebar | `pr_error` | repo-core | 1 | `src/repo/sidebar/pull_requests.rs:143` Write |
| sidebar | `prs` | repo-core | 2 | `src/repo/sidebar/pull_requests.rs:168` Write |
| sidebar | `prs_from_cache` | repo-core | 2 | `src/repo/sidebar/pull_requests.rs:169` Write |
| sidebar | `sidebar_jump` | repo-core | 2 | `src/repo/sidebar/mod.rs:66` MutCall |
| sidebar | `sidebar_reveal` | repo-core | 2 | `src/repo/sidebar/mod.rs:69` MutCall |
| sidebar | `status` | repo-core | 2 | `src/repo/sidebar/pull_requests/worktrees.rs:78` Write |
| sync | `avatars_hinted` | repo-core | 1 | `src/repo/offline.rs:131` Write |
| sync | `busy_op` | repo-core | 5 | `src/repo/push/completion.rs:132` Write |
| sync | `dialog` | repo-core | 8 | `src/repo/https_recovery.rs:56` Write |
| sync | `fetch_time_asked` | repo-core | 1 | `src/repo/offline.rs:138` Write |
| sync | `finished` | repo-core | 1 | `src/repo/https_recovery.rs:133` MutCall |
| sync | `https_checking` | repo-core | 1 | `src/repo/https_recovery.rs:54` Write |
| sync | `https_generation` | repo-core | 1 | `src/repo/https_recovery.rs:52` Write |
| sync | `network_paused` | repo-core | 1 | `src/repo/offline.rs:129` MutBorrow |
| sync | `preflight_pending` | repo-core | 1 | `src/repo/push_preflight.rs:135` MutCall |
| sync | `preflight_selection` | repo-core | 2 | `src/repo/push_preflight.rs:41` MutCall |
| sync | `preflight_timeline` | repo-core | 1 | `src/repo/push_preflight.rs:142` Write |
| sync | `prs_loaded` | repo-core | 1 | `src/repo/offline.rs:132` Write |
| sync | `status` | repo-core | 21 | `src/repo/https_recovery.rs:26` Write |
| terminal | `busy_op` | repo-core | 3 | `src/repo/devcontainer/mod.rs:184` Write |
| terminal | `dialog` | repo-core | 2 | `src/repo/custom_commands/mod.rs:291` Write |
| terminal | `finished` | repo-core | 3 | `src/repo/custom_commands/mod.rs:319` MutCall |
| terminal | `status` | repo-core | 2 | `src/repo/custom_commands/run.rs:18` Write |
| worktrees | `finished` | repo-core | 1 | `src/repo/worktree_tab/mod.rs:260` MutCall |
| worktrees | `parked` | repo-core | 10 | `src/repo/worktree_tab/mod.rs:190` MutCall |
| worktrees | `parked_status` | repo-core | 1 | `src/repo/worktree_tab/mod.rs:265` Write |
| worktrees | `prs` | repo-core | 1 | `src/repo/worktree_tab/mod.rs:234` Write |
| worktrees | `scroll_to` | repo-core | 1 | `src/repo/worktree_tab/mod.rs:247` Write |
| worktrees | `terminals` | terminal | 1 | `src/repo/worktree_tab/mod.rs:172` MutCall |
| worktrees | `worktrees` | repo-core | 1 | `src/repo/worktree_tab/mod.rs:218` Write |

## Analysis gaps

- ambiguous-glob-name: 0
- cfg-inactive: 8
- file-reached-twice: 17
- include-unresolved: 1
- item-macro: 37
- unresolved-path: 31
- `src/resolver/park.rs:426` cfg-inactive: provably inactive for this crate
- `src/resolver/park.rs:429` cfg-inactive: provably inactive for this crate
- `src/resolver/park.rs:433` cfg-inactive: provably inactive for this crate
- `src/resolver/park.rs:437` cfg-inactive: provably inactive for this crate
- `src/resolver/park.rs:441` cfg-inactive: provably inactive for this crate
- `src/resolver/park/durability.rs:197` cfg-inactive: provably inactive for this crate
- `src/resolver/park/effects.rs:9` cfg-inactive: provably inactive for this crate
- `src/resolver/park/effects.rs:12` cfg-inactive: provably inactive for this crate
- `ci/arch-fitness/tests/support/mod.rs:0` file-reached-twice: arch_fitness_test_rules::support
- `src/repo/diff/image_compare/decode.rs:0` file-reached-twice: example_image_memory::decode
- `src/resolver/park.rs:0` file-reached-twice: recovery::resolver::park
- `src/resolver/park/archive.rs:0` file-reached-twice: recovery::resolver::park::archive
- `src/resolver/park/bytes.rs:0` file-reached-twice: recovery::resolver::park::bytes
- `src/resolver/park/discovery.rs:0` file-reached-twice: recovery::resolver::park::discovery
- `src/resolver/park/durability.rs:0` file-reached-twice: recovery::resolver::park::durability
- `src/resolver/park/effects.rs:0` file-reached-twice: recovery::resolver::park::effects
- `src/resolver/park/guards.rs:0` file-reached-twice: recovery::resolver::park::guards
- `src/resolver/park/journal.rs:0` file-reached-twice: recovery::resolver::park::journal
- `src/resolver/park/journal/receipts.rs:0` file-reached-twice: recovery::resolver::park::journal::receipts
- `src/resolver/park/journal/tests.rs:0` file-reached-twice: recovery::resolver::park::journal::tests
- `src/resolver/park/locking.rs:0` file-reached-twice: recovery::resolver::park::locking
- `src/resolver/park/locking/tests.rs:0` file-reached-twice: recovery::resolver::park::locking::tests
- `src/resolver/park/publication.rs:0` file-reached-twice: recovery::resolver::park::publication
- `src/resolver/park/restore.rs:0` file-reached-twice: recovery::resolver::park::restore
- `src/resolver/park/transaction.rs:0` file-reached-twice: recovery::resolver::park::transaction
- `src/i18n/mod.rs:16` include-unresolved: concat ! (env ! ("OUT_DIR") , "/gitcraft_catalogs.rs")

Unparsed macro bodies (paths inside are unseen): `json!` 437, `thread_local!` 34, `Token!` 8, `select!` 3, `define_class!` 2, `msg_send!` 2, `preferences!` 1, `sel!` 1

Ignored files: 1

