//! Private reference-transaction hook generation and original-hook relay.
use super::*;

pub(in super::super) struct HookScript<'a> {
    pub(in super::super) original: &'a str,
    pub(in super::super) branch: &'a str,
    pub(in super::super) old: &'a str,
    pub(in super::super) new: &'a str,
    pub(in super::super) index: &'a str,
    pub(in super::super) shared: Option<&'a str>,
    pub(in super::super) affected: &'a [String],
    pub(in super::super) proof: &'a str,
    pub(in super::super) needs_checkout: bool,
}

pub(in super::super) fn script(args: HookScript<'_>) -> String {
    let HookScript {
        original,
        branch,
        old,
        new,
        index,
        shared,
        affected,
        proof,
        needs_checkout,
    } = args;
    let original_dir = original
        .rsplit_once('/')
        .map_or(original, |(directory, _)| directory);
    let paths = affected
        .iter()
        .map(|path| remote::sh_quote(path))
        .collect::<Vec<_>>()
        .join(" ");
    let shared_check = shared
        .map(|path| {
            format!(
                "cmp -s -- {} \"$directory/before.shared-index\" || exit 1\n",
                remote::sh_quote(path)
            )
        })
        .unwrap_or_default();
    let payload = format!(
        r#"case "$1" in preparing|prepared|committed|aborted) ;; *) exit 1 ;; esac
directory=$(dirname "$0")
command -v cmp >/dev/null && command -v cp >/dev/null || exit 1
if [ "$(cat "$directory/mode")" = probe ]; then
    cat >"$directory/probe-updates-$1" || exit 1
    printf 'probe\n' >"$directory/probe-$1.ok" || exit 1
    exit 0
fi
updates="$directory/updates-$1"
cat >"$updates" || exit 1
if [ "$1" = prepared ]; then
    matched=0
    while read -r before after reference extra; do
        [ -z "$extra" ] || exit 1
        [ "$reference" = {branch} ] || exit 1
        [ "$before" = {old} ] && [ "$after" = {new} ] || exit 1
        matched=$((matched+1))
    done <"$updates"
    [ "$matched" = 1 ] || exit 1
    [ "$(git symbolic-ref -q HEAD)" = {branch} ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {old} ] || exit 1
    git hook run --ignore-missing --to-stdin="$updates" reference-transaction -- "$@" || exit $?
    [ "$(git symbolic-ref -q HEAD)" = {branch} ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {old} ] || exit 1
    cmp -s -- {index} "$directory/before.index" || exit 1
    {shared_check}
    GIT_OPTIONAL_LOCKS=0 GIT_INDEX_FILE="$directory/check-before.index" git -c core.fsmonitor=false -c core.trustctime=true -c core.ignoreStat=false diff --quiet --no-ext-diff --no-textconv --ignore-submodules=none -- || exit 1
    if [ {needs_checkout} = true ]; then
        git --literal-pathspecs -c core.fsmonitor=false status --porcelain=v1 -z --untracked-files=all --ignored=matching -- {paths} >"$directory/affected-status" || exit 1
        [ ! -s "$directory/affected-status" ] || exit 1
    fi
    printf '%s' {proof} >"$directory/prepared.ok" || exit 1
    if [ {needs_checkout} = true ]; then
        printf 'attempted\n' >"$directory/checkout-attempted" || exit 1
        git -c core.fsmonitor=false read-tree -u -m {old} {new} || exit 1
    fi
    [ "$(git symbolic-ref -q HEAD)" = {branch} ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {old} ] || exit 1
    git -c core.fsmonitor=false diff-index --cached --quiet --no-ext-diff --no-textconv {new} -- || exit 1
    GIT_OPTIONAL_LOCKS=0 GIT_INDEX_FILE="$directory/check-after.index" git -c core.fsmonitor=false -c core.trustctime=true -c core.ignoreStat=false diff --quiet --no-ext-diff --no-textconv --ignore-submodules=none -- || exit 1
    printf '%s' {proof} >"$directory/checkout.ok" || exit 1
elif [ "$1" = committed ]; then
    git hook run --ignore-missing --to-stdin="$updates" reference-transaction -- "$@" || exit $?
    [ "$(git symbolic-ref -q HEAD)" = {branch} ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {new} ] || exit 1
    git -c core.fsmonitor=false diff-index --cached --quiet --no-ext-diff --no-textconv {new} -- || exit 1
    GIT_OPTIONAL_LOCKS=0 GIT_INDEX_FILE="$directory/check-committed.index" git -c core.fsmonitor=false -c core.trustctime=true -c core.ignoreStat=false diff --quiet --no-ext-diff --no-textconv --ignore-submodules=none -- || exit 1
    printf '%s' {proof} >"$directory/committed.ok" || exit 1
else
    git hook run --ignore-missing --to-stdin="$updates" reference-transaction -- "$@"
fi
"#,
        branch = remote::sh_quote(branch),
        old = remote::sh_quote(old),
        new = remote::sh_quote(new),
        index = remote::sh_quote(index),
        proof = remote::sh_quote(proof)
    );
    // The outer command's sole -c selects this wrapper. Remove that inherited parameter
    // before relaying the original hook; preserve the actual profile's remaining config.
    format!(
        "#!/bin/sh\numask 077\nunset GIT_CONFIG_PARAMETERS\nexec env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0={} sh -c {} \"$0\" \"$@\"\n",
        remote::sh_quote(original_dir),
        remote::sh_quote(&payload)
    )
}
