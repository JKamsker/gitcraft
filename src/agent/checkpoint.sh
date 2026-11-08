# gitcraft checkpoint, for a machine without gitcraft (WSL, SSH): written into the repository's
# git folder by gitcraft and run by the coding agent's hooks. After each turn it saves the
# worktree (uncommitted and untracked files included, its own index untouched) as a commit
# under refs/gitcraft/agent/<session>/<n>, as `gitcraft checkpoint` does. It never fails the
# agent: it always exits with 0.
if [ "$1" = --codex ]; then gitcraft_json=$2; else gitcraft_json=$(cat); fi
gitcraft_json=$(printf '%s' "$gitcraft_json" | tr '\n\r' '  ')
# JSON's escapes undone, from left to right: `\\` first set aside (as \001), so `C:\\new`
# stays `C:\new`; a \uXXXX is left as it is, but for a control character, dropped.
gitcraft_unescape() {
  sed 's/\\\\/\x01/g; s/\\u00[01][0-9a-fA-F]//g; s/\\n/\n/g; s/\\t/\t/g; s/\\r//g; s/\\"/"/g; s/\\\//\//g; s/\x01/\\/g'
}
# A string field of the JSON (its last one of that name), unescaped.
gitcraft_field() {
  printf '%s' "$gitcraft_json" |
    sed -n 's/.*"'"$1"'"[[:space:]]*:[[:space:]]*"\(\([^"\\]\|\\.\)*\)".*/\1/p' | head -n 1 |
    gitcraft_unescape
}
if [ "$1" = --codex ]; then
  gitcraft_event=$(gitcraft_field type)
  gitcraft_session=$(gitcraft_field thread-id)
  [ -n "$gitcraft_session" ] || gitcraft_session=codex
  # The last of "input-messages".
  gitcraft_prompt=$(printf '%s' "$gitcraft_json" |
    sed -n 's/.*"input-messages"[[:space:]]*:[[:space:]]*\[\(\([^]"]\|"\([^"\\]\|\\.\)*"\)*\)\].*/\1/p' |
    grep -o '"\([^"\\]\|\\.\)*"' | tail -n 1 | sed 's/^"//; s/"$//' |
    gitcraft_unescape)
else
  gitcraft_event=$(gitcraft_field hook_event_name)
  gitcraft_session=$(gitcraft_field session_id)
  gitcraft_prompt=$(gitcraft_field prompt)
fi
gitcraft_cwd=$(gitcraft_field cwd)
[ -n "$gitcraft_cwd" ] && cd "$gitcraft_cwd" 2>/dev/null
gitcraft_top=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$gitcraft_top" || exit 0
# The session as one component of a ref, as gitcraft makes it.
gitcraft_session=$(printf '%s' "$gitcraft_session" | LC_ALL=C tr -c 'A-Za-z0-9._-' '-' | cut -c1-64)
[ -n "$gitcraft_session" ] || gitcraft_session=session
case $gitcraft_session in .* | *. | *.lock) gitcraft_session="s${gitcraft_session}s" ;; esac
gitcraft_session=$(printf '%s' "$gitcraft_session" | sed 's/\.\./-/g')
gitcraft_common=$(git rev-parse --git-common-dir) || exit 0
gitcraft_prompts="$gitcraft_common/gitcraft/agent-prompts"
if [ "$gitcraft_event" = UserPromptSubmit ]; then
  mkdir -p -- "$gitcraft_prompts" && printf '%s' "$gitcraft_prompt" > "$gitcraft_prompts/$gitcraft_session"
  exit 0
fi
if [ -z "$gitcraft_prompt" ] && [ -f "$gitcraft_prompts/$gitcraft_session" ]; then
  gitcraft_prompt=$(cat -- "$gitcraft_prompts/$gitcraft_session")
fi
rm -f -- "$gitcraft_prompts/$gitcraft_session"
# The worktree as a tree, through an index of its own seeded from the worktree's.
gitcraft_own=$(git rev-parse --git-path index)
gitcraft_index=$(git rev-parse --git-path "gitcraft-checkpoint-$$.index")
if ! cp -p -f -- "$gitcraft_own" "$gitcraft_index" 2>/dev/null; then
  git rev-parse -q --verify HEAD >/dev/null && GIT_INDEX_FILE=$gitcraft_index git read-tree HEAD
fi
gitcraft_tree=
GIT_INDEX_FILE=$gitcraft_index git add -A -- . && gitcraft_tree=$(GIT_INDEX_FILE=$gitcraft_index git write-tree)
rm -f -- "$gitcraft_index"
[ -n "$gitcraft_tree" ] || exit 0
gitcraft_refs="refs/gitcraft/agent/$gitcraft_session"
gitcraft_last=$(git for-each-ref --format='%(refname)' "$gitcraft_refs/" | sed 's|.*/||' | grep -E '^[0-9]+$' | sort -n | tail -n 1)
if [ -n "$gitcraft_last" ]; then
  gitcraft_parent=$(git rev-parse "$gitcraft_refs/$gitcraft_last") || exit 0
  [ "$(git rev-parse "$gitcraft_parent^{tree}")" = "$gitcraft_tree" ] && exit 0
  gitcraft_n=$((gitcraft_last + 1))
  set -- -p "$gitcraft_parent"
else
  gitcraft_n=1
  if gitcraft_head=$(git rev-parse -q --verify HEAD); then set -- -p "$gitcraft_head"; else set --; fi
fi
# The prompt without the spaces around it; its first line names the turn.
gitcraft_prompt=$(printf '%s\n' "$gitcraft_prompt" | sed '/[^[:space:]]/,$!d' | sed -e ':a' -e '/^[[:space:]]*$/{$d;N;ba' -e '}' | sed -e '1s/^[[:space:]]*//' -e '$s/[[:space:]]*$//')
gitcraft_subject=$(printf '%s\n' "$gitcraft_prompt" | head -n 1 | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
[ -n "$gitcraft_subject" ] || gitcraft_subject="Turn $gitcraft_n"
gitcraft_msg=$(printf '%s\n\n%s\n\nGitcraft-Session: %s\nGitcraft-Worktree: %s\nGitcraft-Turn: %s' \
  "$gitcraft_subject" "$gitcraft_prompt" "$gitcraft_session" "$gitcraft_top" "$gitcraft_n")
gitcraft_commit=$(GIT_AUTHOR_NAME='gitcraft checkpoint' GIT_AUTHOR_EMAIL=checkpoint@gitcraft.invalid \
  GIT_COMMITTER_NAME='gitcraft checkpoint' GIT_COMMITTER_EMAIL=checkpoint@gitcraft.invalid \
  git commit-tree "$gitcraft_tree" "$@" -m "$gitcraft_msg") || exit 0
git update-ref "$gitcraft_refs/$gitcraft_n" "$gitcraft_commit" "" 2>/dev/null
exit 0
