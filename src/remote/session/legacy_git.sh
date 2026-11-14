#!/bin/sh
# Put first in PATH by gitcraft only where this machine's Git predates --no-lazy-fetch
# (Git 2.44). That Git can't be told not to download promised objects, so the option is
# dropped only in repositories without partial-clone or promisor configuration, where no
# object is ever promised; elsewhere the command is refused instead of fetching.
real=$GITCRAFT_LEGACY_GIT
case $real in
/*) ;;
*)
    printf '%s\n' 'gitcraft: the --no-lazy-fetch compatibility shim has no Git to run' >&2
    exit 126
    ;;
esac
n=0 found=0 take=0 keep=
for a do
    if [ "$take" = 1 ]; then
        take=0
    else
        case $a in
        --no-lazy-fetch)
            found=1
            n=$((n + 1))
            continue
            ;;
        -C | -c | --git-dir | --work-tree | --namespace | --config-env | --attr-source) take=1 ;;
        -*) ;;
        *) break ;;
        esac
    fi
    n=$((n + 1))
    eval "l$n=\$a"
    keep="$keep \"\$l$n\""
done
[ "$found" = 1 ] || exec "$real" "$@"
pattern='^(extensions\.partialclone|remote\..*\.promisor)$'
err=$(eval "\"\$real\" $keep config --get-regexp \"\$pattern\"" 2>&1 >/dev/null </dev/null)
code=$?
case $code in
0)
    printf 'gitcraft: this repository is a partial clone and %s on this machine cannot be kept from downloading missing objects. Install Git 2.44 or newer here.\n' \
        "$("$real" --version 2>/dev/null </dev/null)" >&2
    exit 125
    ;;
1) ;;
*)
    printf '%s\n' "$err" >&2
    exit "$code"
    ;;
esac
shift "$n"
eval "exec \"\$real\" $keep \"\$@\""
