#!/usr/bin/env bash
# Starts a throwaway Forgejo or Gitea on 127.0.0.1 for gitcraft's live tests (src/forge/gitea/live.rs):
# SQLite, no SSH, no mail, two users — `author` (an admin) and `reviewer` — each with an API token.
#
#   ci/forge-server.sh forgejo 3000 forges.env
#   ci/forge-server.sh gitea   3001 forges.env
#   set -a; . ./forges.env; set +a
#   cargo test live -- --ignored
#
# What the tests need is appended to the env file (default: printed):
#   GITCRAFT_<KIND>_URL, GITCRAFT_<KIND>_AUTHOR_TOKEN, GITCRAFT_<KIND>_REVIEWER_TOKEN
# The server keeps running in the background; its pid is in <work dir>/pid, its log in <work dir>/log.
#
# FORGEJO_VERSION / GITEA_VERSION pick the release, FORGE_CACHE where binaries are kept,
# FORGE_WORK where the servers' data goes.
set -euo pipefail

kind=${1:?forgejo or gitea}
port=${2:?port}
env_file=${3:-/dev/stdout}

case "$(uname -m)" in
  x86_64 | amd64) arch=amd64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *) echo "unsupported machine: $(uname -m)" >&2; exit 1 ;;
esac

case "$kind" in
  forgejo)
    version=${FORGEJO_VERSION:-17.0.1}
    url="https://codeberg.org/forgejo/forgejo/releases/download/v$version/forgejo-$version-linux-$arch"
    ;;
  gitea)
    version=${GITEA_VERSION:-28.3.0}
    url="https://dl.gitea.com/gitea/$version/gitea-$version-linux-$arch"
    ;;
  *) echo "unknown forge: $kind (forgejo or gitea)" >&2; exit 1 ;;
esac

cache=${FORGE_CACHE:-$HOME/.cache/gitcraft-forges}
bin="$cache/$kind-$version-$arch"
mkdir -p "$cache"
if [ ! -x "$bin" ]; then
  echo "Downloading $kind $version" >&2
  curl -fsSL --retry 3 -o "$bin.part" "$url"
  expected=$(curl -fsSL --retry 3 "$url.sha256" | awk '{print $1}')
  actual=$(sha256sum "$bin.part" | awk '{print $1}')
  if [ "$expected" != "$actual" ]; then
    echo "checksum mismatch for $url: expected $expected, got $actual" >&2
    rm -f "$bin.part"
    exit 1
  fi
  chmod +x "$bin.part"
  mv "$bin.part" "$bin"
fi

work="${FORGE_WORK:-${TMPDIR:-/tmp}/gitcraft-forges}/$kind-$port"
if [ -f "$work/pid" ]; then
  kill "$(cat "$work/pid")" 2>/dev/null || true
fi
rm -rf "$work"
mkdir -p "$work/custom/conf" "$work/data" "$work/repos" "$work/log"
ini="$work/custom/conf/app.ini"
root="http://127.0.0.1:$port"

# These forges refuse to run as root unless told; a CI runner isn't root, a container may be.
as_root=""
if [ "$(id -u)" = 0 ]; then
  as_root="I_AM_BEING_UNSAFE_RUNNING_AS_ROOT = true"
fi

cat > "$ini" <<EOF
APP_NAME = gitcraft test $kind
RUN_MODE = prod
WORK_PATH = $work
$as_root

[database]
DB_TYPE = sqlite3
PATH = $work/data/forge.db

[server]
PROTOCOL = http
HTTP_ADDR = 127.0.0.1
HTTP_PORT = $port
ROOT_URL = $root/
DISABLE_SSH = true
START_SSH_SERVER = false
OFFLINE_MODE = true
LFS_START_SERVER = false
APP_DATA_PATH = $work/data

[repository]
ROOT = $work/repos
DEFAULT_BRANCH = main

[security]
INSTALL_LOCK = true
SECRET_KEY = $("$bin" generate secret SECRET_KEY)
INTERNAL_TOKEN = $("$bin" generate secret INTERNAL_TOKEN)

[oauth2]
JWT_SECRET = $("$bin" generate secret JWT_SECRET)

[service]
DISABLE_REGISTRATION = true
REQUIRE_SIGNIN_VIEW = false

[mailer]
ENABLED = false

[actions]
ENABLED = false

[log]
MODE = file
LEVEL = Warn
ROOT_PATH = $work/log
EOF

forge() { "$bin" --work-path "$work" --config "$ini" "$@"; }

forge migrate >"$work/log/setup" 2>&1
for user in author reviewer; do
  admin=""
  [ "$user" = author ] && admin="--admin"
  forge admin user create $admin --username "$user" --email "$user@example.org" \
    --password "gitcraft-$user-$RANDOM$RANDOM" --must-change-password=false >>"$work/log/setup" 2>&1
done
token() {
  forge admin user generate-access-token --username "$1" --token-name "gitcraft-tests" \
    --scopes all --raw 2>>"$work/log/setup" | tail -n 1
}
author_token=$(token author)
reviewer_token=$(token reviewer)

forge web >"$work/log/web" 2>&1 &
echo $! >"$work/pid"
for _ in $(seq 1 60); do
  if curl -fsS "$root/api/v1/version" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$(cat "$work/pid")" 2>/dev/null; then
    echo "$kind stopped while starting:" >&2
    cat "$work/log/setup" "$work/log/web" "$work/log/"*.log >&2 2>/dev/null || true
    exit 1
  fi
  sleep 1
done
if ! curl -fsS "$root/api/v1/version" >/dev/null; then
  echo "$kind didn't answer on $root" >&2
  cat "$work/log/web" >&2
  exit 1
fi
echo "$kind $version is up on $root ($(curl -fsS "$root/api/v1/version"))" >&2

# Throwaway tokens of a throwaway server, but they still stay out of CI logs.
if [ "${GITHUB_ACTIONS:-}" = true ]; then
  echo "::add-mask::$author_token"
  echo "::add-mask::$reviewer_token"
fi

upper=$(echo "$kind" | tr '[:lower:]' '[:upper:]')
{
  echo "GITCRAFT_${upper}_URL=$root"
  echo "GITCRAFT_${upper}_AUTHOR_TOKEN=$author_token"
  echo "GITCRAFT_${upper}_REVIEWER_TOKEN=$reviewer_token"
} >>"$env_file"
