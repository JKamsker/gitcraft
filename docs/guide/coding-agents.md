# Coding agents

gitcraft is built to work alongside Claude Code, Codex and other agents: an MCP server, agent tasks in their own worktrees, per-turn checkpoints and tools to compare attempts.

## Agent tasks and worktrees

- _Start agent task…_ (the + of AGENTS on the left, or the palette) makes a branch and a
  worktree beside the repository, opens it and starts a terminal there running the agent you
  pick: Claude Code, Codex, Gemini CLI, Cursor Agent, Aider or a command of your own, the task
  given as its first message when the agent takes one. If the agent isn't installed there, the
  terminal says so. AGENTS lists the tasks with their uncommitted changes; click one to
  open its worktree and review what the agent did with the normal diff.
- _Worktree setup…_ (palette): a recipe per repository that runs in every worktree gitcraft
  makes (the Add worktree dialog, a pull request's _Open in worktree_, agent tasks), in a
  terminal with its output: it copies or links ignored files from the main worktree
  (`.env.local`, `node_modules`), sets variables per worktree
  (`PORT=$((3000 + GITCRAFT_WORKTREE_INDEX))`, a `CARGO_TARGET_DIR` of its own; the main
  worktree is number 0, the others keep the number they were given) and runs a setup command
  (`npm ci`). A step that fails says so; the setup command and the agent are then not run. It runs with `sh` on the repository's machine (Git's own `sh` on Windows),
  WSL and SSH included; the script it makes can be read in the editor, and tried on a worktree
  before saving. Kept in `.git/gitcraft/`, never committed.

## Agent turns as review steps

- Agent turns as review steps: _Agent turns: save each turn in this worktree_ (palette), or
  _Save each turn to review on its own_ when starting a task, installs Claude Code's
  `UserPromptSubmit` / `Stop` hooks (`.claude/settings.local.json`) and Codex's `notify`
  (`.codex/config.toml`, read for trusted projects), both kept out of git through
  `info/exclude`. After each turn they run `gitcraft checkpoint`, which saves the worktree as
  it is (uncommitted and untracked files, its own index untouched) under
  `refs/gitcraft/agent/<session>/<n>`, a turn that changed nothing skipped. _Uncommitted
  changes_ then shows AGENT TURNS, each named by its prompt: click one to see what that turn
  changed; its menu compares it with the files as they are now. The latest session shows,
  earlier ones fold away (named by their first prompt); a session's menu deletes its turns.
  Hooks take effect the next time the agent starts in the worktree. On a WSL or SSH machine, where gitcraft isn't,
  the hooks run a `sh` script gitcraft writes into the repository's git folder. The turns are
  not history: the graph leaves them out.

## Hand it to the agent

- Hand it to the agent: click a line's number in a diff (Alt+click in a diff of uncommitted
  changes, where a click picks the line to stage it) to write a note for the agent on it. Notes
  are kept per worktree until they are sent, never published, and collect in NOTES FOR THE
  AGENT under the big area; _Send to agent_ types them into the worktree's agent terminal (a
  terminal named after the agent, or the one picked beside the button) as one message, pasted
  for you to send with Enter, and clears them. The same _Send to agent_ is on a conflict (each
  conflict's three sides), on a failing CI run's log (its end) and on a pull request's thread
  (with its file and line). Nothing is typed into a program that hasn't asked for pastes.

## Bake-off: compare attempts

- _Bake-off: compare attempts…_ (palette, or _Compare them_ under AGENTS): pick 2–4 worktrees or
  branches that tried the same task (the agent tasks come picked when there are 2–4 of them).
  A grid compares them with the commit they all start from (their merge base; one that started
  from a newer commit is named):
  the files each touched, its size, the dependencies it adds (Cargo, npm, Go, Python, Ruby,
  Composer manifests) and whether it builds (a command you type, kept for the repository, run
  in a terminal in each worktree). A worktree's uncommitted work is compared too, read through
  an index of gitcraft's own. Click a file to see it side by side across the attempts that
  changed it; take it whole from one (click again to take it back), or tick hunks of several
  (a binary file only goes whole), and _Assemble_ makes a new branch from the base with what
  you picked, without checking anything out; the commit message lists where each file came
  from, and _Show in graph_ goes to it. Hunks of two attempts that change the same lines are
  flagged, and Assemble refuses them.

## MCP server (`gitcraft mcp`)

- `gitcraft mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server over
  stdio, for Claude Code, Codex and other agents. Add it once:

  ```sh
  claude mcp add --scope user gitcraft -- gitcraft mcp
  codex mcp add gitcraft -- gitcraft mcp
  ```

  (or, in Codex's `~/.codex/config.toml`, `[mcp_servers.gitcraft]` with `command = "gitcraft"`
  and `args = ["mcp"]`). Settings → Agents shows the same commands with this copy's own path when
  `gitcraft` isn't on the PATH. The agent starts it in the repository it works in; `--repo
  <path>` names another one (also one in WSL or over SSH, as gitcraft names it:
  `ssh://host/path`).
- Reading tools work on their own, with no window open: `log` (the graph: commits with their
  branches and tags, paged, or one file's history), `show_commit`, `status`, `diff` (working
  tree, index or a commit, paged by bytes), `blame` (a range of lines), `branches` (with
  remotes, tags, stashes and worktrees), `conflicts`, `ci_status` (runs, jobs and the failed log
  through `gh` / `glab`) and `pull_requests`. Paths must stay inside the repository, revisions
  can't be taken for options, and a remote's URL never carries a token out.
- Changing tools run in the gitcraft window that has the repository open: `stage`, `unstage`,
  `commit`, `create_branch`, `rename_branch`, `checkout`, `stash`, `cherry_pick`, `revert`,
  `reset` (soft, mixed) and `move_commit`. Each is a step of that tab's undo list, marked with a
  robot on the undo button and named in its tooltip, so Ctrl+Z also takes back what the agent
  did. With no window for the repository, they fail and say to open it.
- What can't be undone, or is easy to regret, waits on an _Approve / Deny_ card in the window
  that names the agent, says what will happen and shows the git commands: `push` (forced or
  not), `delete_branch`, a `hard` reset, `drop_commit`, and staging a conflicted file (which ends
  the conflict). The call waits for the answer; after two minutes it is denied automatically.
- An approval can take up to two minutes, longer than some agents wait for a tool by default:
  give the server more time there (Codex: `tool_timeout_sec = 180` under
  `[mcp_servers.gitcraft]`). A call the agent cancels, or an agent that quits, takes its card
  away, and what hadn't started doesn't run.
- `gitcraft mcp` has no window of its own to ask ssh's questions in: for a repository over SSH
  it signs in with keys (an agent or a key without a passphrase) only. An agent running inside
  WSL starts the Linux gitcraft, which can't reach a window on Windows: start it from Windows
  with `--repo \\wsl.localhost\<distro>\path` instead.
- Settings → Agents turns agents' changes off; the reading tools still work. `gitcraft mcp`
  reaches the window the way a second start of gitcraft does (see [one window](repositories-and-tabs.md#one-window)): over
  127.0.0.1, with the running window's random token from a file only you can read, believed only
  while that window holds its lock. An agent's change is a request of its own kind there, and
  its connection stays open until the window answers.
