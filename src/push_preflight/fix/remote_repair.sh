# All paths/IDs arrive as quoted request environment values, never evaluated as code.
# No background job, PID lookup, signal, timeout controller or speculative cleanup.
umask 077
D=$GITCRAFT_REPAIR_DIR
G=$GITCRAFT_REPAIR_GUARD
N=$GITCRAFT_REPAIR_NONCE
B=$GITCRAFT_REPAIR_BRANCH
O=$GITCRAFT_REPAIR_OLD
A=$GITCRAFT_REPAIR_NEW
I=$GITCRAFT_REPAIR_INDEX
S=$GITCRAFT_REPAIR_SHARED
mkdir -m 700 -- "$D" || exit 1
printf '%s\n' "$N" >"$D/owner" || exit 1
mkdir -m 700 -- "$G" || exit 1
printf '%s\n' "$N" >"$G/owner" || exit 1
# A terminal receipt is written only after the foreground native Git child returned.
# Unknown transport outcomes leave this receipt and guard intact until that happens.
finish() {
    code=$1
    actual=$(git --no-lazy-fetch rev-parse --verify HEAD 2>/dev/null) || actual=unknown
    printf 'gitcraft-owned-repair-v1\n%s\n%s\n%s\n%s\n' "$N" "$code" "$GITCRAFT_REPAIR_KIND" "$actual" >"$D/outcome.tmp" && mv -- "$D/outcome.tmp" "$D/outcome" || exit 1
    if [ "$(cat "$G/owner")" = "$N" ]; then
        rm -- "$G/owner" && rmdir -- "$G" || exit 1
    else
        exit 1
    fi
    if [ "$code" = 0 ]; then printf 'complete:%s\n' "$N"; fi
    exit "$code"
}
refuse() { printf '%s\n' "$1" >"$D/refusal"; finish 1; }
copy_index() {
    [ -f "$1" ] || return 1
    dd if="$1" of="$2" bs=65536 count=1025 2>/dev/null || return 1
    [ "$(wc -c <"$2")" -le 67108864 ]
}
[ "$(git --version)" = "$GITCRAFT_REPAIR_VERSION" ] || refuse 'Git version changed'
[ "$(git --no-lazy-fetch rev-parse --show-ref-format)" = "$GITCRAFT_REPAIR_REFS" ] || refuse 'Reference backend changed'
[ "$(git --no-lazy-fetch rev-parse --show-object-format)" = "$GITCRAFT_REPAIR_OBJECTS" ] || refuse 'Object backend changed'
[ "$(git --no-lazy-fetch symbolic-ref -q HEAD)" = "$B" ] || refuse 'Checkout changed'
[ "$(git --no-lazy-fetch rev-parse --verify HEAD)" = "$O" ] || refuse 'HEAD changed'
git --no-lazy-fetch symbolic-ref -q "$B" >"$D/symbolic-branch"
[ "$?" = 1 ] || refuse 'Repair requires a direct branch'
copy_index "$I" "$D/before.index" || refuse 'Could not save bounded index recovery'
shared=$(GIT_INDEX_FILE="$D/before.index" git --no-lazy-fetch rev-parse --path-format=absolute --shared-index-path) || refuse 'Saved index dependency is unreadable'
[ "$shared" = "$S" ] || refuse 'Index dependency rotated; recheck before repair'
if [ -n "$S" ]; then copy_index "$S" "$D/before.shared-index" || refuse 'Could not save shared-index dependency'; fi
printf 'Guarded remote repair recovery\nBranch: %s\nOriginal: %s\nRewritten: %s\nIndex path: %s\nShared-index dependency: %s\n\nInspect HEAD, status, diff and diff --cached. Save concurrent edits before deliberate recovery. Saved indexes are evidence, not permission to overwrite later work. A missing outcome is NOT proof the host stopped.\n' "$B" "$O" "$A" "$I" "$S" >"$D/recovery.txt" || refuse 'Recovery write failed'
# Match the local restrictions without parsing filenames through shell words.
git --no-lazy-fetch -c core.quotePath=true ls-files -v >"$D/index-flags" || refuse 'Index flags unreadable'
LC_ALL=C awk 'substr($0,1,2)!="H " {bad=1} END {exit bad}' "$D/index-flags" || refuse 'Unsupported index flags'
cat >"$D/reference-transaction" || refuse 'Wrapper write failed'
chmod 700 "$D/reference-transaction" || refuse 'Wrapper permissions failed'
# Same backend/format scratch proof, hook stdin command and native HEAD operation.
C=$D/capability.git
git -c core.hooksPath= init --bare --template= --object-format="$GITCRAFT_REPAIR_OBJECTS" --ref-format="$GITCRAFT_REPAIR_REFS" "$C" >"$D/capability.log" 2>&1 || refuse 'Capability init failed'
: >"$D/empty-tree"
tree=$(git -C "$C" -c core.hooksPath= hash-object -w --no-filters -t tree -- "$D/empty-tree") || refuse 'Capability tree failed'
cap_commit() {
    GIT_AUTHOR_NAME='Gitcraft capability' GIT_AUTHOR_EMAIL=probe@example.invalid GIT_COMMITTER_NAME='Gitcraft capability' GIT_COMMITTER_EMAIL=probe@example.invalid git -C "$C" -c core.hooksPath= commit-tree --no-gpg-sign "$tree" -m "$1"
}
co=$(cap_commit Before) || refuse 'Capability commit failed'
cn=$(cap_commit After) || refuse 'Capability commit failed'
cb=refs/heads/gitcraft-capability
git -C "$C" -c core.hooksPath= symbolic-ref HEAD "$cb" && git -C "$C" -c core.hooksPath= update-ref "$cb" "$co" || refuse 'Capability HEAD failed'
printf probe >"$D/mode"
printf '#!/bin/sh\nd=$(dirname "$0")\ncat >"$d/hook-command.stdin" || exit 1\nprintf "probe\\n" >"$d/hook-command.ok"\n' >"$D/pre-push"
chmod 700 "$D/pre-push" || refuse 'Capability command write failed'
printf 'owned capability input\n' >"$D/hook-input"
git -C "$C" -c core.hooksPath="$D" hook run --ignore-missing --to-stdin="$D/hook-input" pre-push >>"$D/capability.log" 2>&1 || refuse 'Capability hook command failed'
printf 'probe\n' >"$D/probe.expected"
cmp -s -- "$D/probe.expected" "$D/hook-command.ok" && cmp -s -- "$D/hook-input" "$D/hook-command.stdin" || refuse 'Capability command acknowledgement absent'
printf 'start\nupdate HEAD %s %s\nprepare\ncommit\n' "$cn" "$co" | git -C "$C" -c core.hooksPath="$D" update-ref --stdin >>"$D/capability.log" 2>&1 || refuse 'Native capability refused'
cmp -s -- "$D/probe.expected" "$D/probe-prepared.ok" && cmp -s -- "$D/probe.expected" "$D/probe-committed.ok" || refuse 'Native capability acknowledgements absent'
printf '%s %s %s\n' "$co" "$cn" "$cb" >"$D/probe-updates.expected"
cmp -s -- "$D/probe-updates.expected" "$D/probe-updates-prepared" || refuse 'Native capability target was not exact'
[ "$(git -C "$C" symbolic-ref -q HEAD)" = "$cb" ] && [ "$(git -C "$C" rev-parse --verify HEAD)" = "$cn" ] || refuse 'Native capability result mismatch'
printf repair >"$D/mode"
if [ "$GITCRAFT_REPAIR_KIND" = merge ]; then
    [ "$(git --no-lazy-fetch symbolic-ref -q HEAD)" = "$B" ] && [ "$(git --no-lazy-fetch rev-parse --verify HEAD)" = "$O" ] || refuse 'Checkout moved before merge'
    cmp -s -- "$I" "$D/before.index" || refuse 'Index changed before merge'
    git --no-lazy-fetch status --porcelain=v1 --untracked-files=all >"$D/merge-status" || refuse 'Working state unreadable'
    [ ! -s "$D/merge-status" ] || refuse 'Working state changed before merge'
    printf 'native merge started\n' >"$D/phase"
    git --no-lazy-fetch merge --no-edit --no-autostash "$GITCRAFT_REPAIR_BASE" >/dev/null 2>&1
    finish "$?"
fi
for name in before after committed; do
    target=$A; [ "$name" != before ] || target=$O
    GIT_INDEX_FILE="$D/check-$name.index" git --no-lazy-fetch -c core.hooksPath= -c core.sparseCheckout=false read-tree "$target" || refuse 'Private validation index failed'
done
printf 'native rewrite started\n' >"$D/phase"
printf 'start\nupdate HEAD %s %s\nprepare\ncommit\n' "$A" "$O" | git --no-lazy-fetch -c core.hooksPath="$D" update-ref -m 'gitcraft push preflight fix' --stdin >/dev/null 2>&1
code=$?
[ "$code" = 0 ] || finish "$code"
printf '%s\n%s\n%s\n' "$B" "$O" "$A" >"$D/repair.expected"
for name in prepared checkout committed; do cmp -s -- "$D/repair.expected" "$D/$name.ok" || finish 1; done
finish 0
