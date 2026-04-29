#!/usr/bin/env bash
# Bulk: set the two Cloudflare secrets every workload's etna-publish.yml
# expects on every stable workload listed in `docs/workloads/index.json`.
#
# Idempotent — overwrites existing secrets with the same values.
#
# Usage:
#   scripts/set-cf-secrets-on-workloads.sh \
#     --token-stdin \
#     --account-id <account-id> \
#     [--dry-run] [--limit N]
#
# Examples:
#   # paste the token securely from stdin (no shell history)
#   echo -n "$CLOUDFLARE_TOKEN" | scripts/set-cf-secrets-on-workloads.sh \
#     --token-stdin --account-id abcdef123
#
#   # dry-run to see which repos would get touched
#   scripts/set-cf-secrets-on-workloads.sh --token-stdin --account-id x --dry-run
#
# Required env:
#   GH_TOKEN with `repo` scope (or `gh auth login`).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INDEX_JSON="$SCRIPT_DIR/../docs/workloads/index.json"

TOKEN=""
ACCOUNT_ID=""
DRY_RUN=0
LIMIT=999
TOKEN_FROM_STDIN=0

while [ $# -gt 0 ]; do
  case "$1" in
    --token)        TOKEN="$2"; shift 2 ;;
    --token-stdin)  TOKEN_FROM_STDIN=1; shift ;;
    --account-id)   ACCOUNT_ID="$2"; shift 2 ;;
    --dry-run)      DRY_RUN=1; shift ;;
    --limit)        LIMIT="$2"; shift 2 ;;
    -h|--help)      sed -n '2,21p' "$0"; exit 0 ;;
    *)              echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [ "$TOKEN_FROM_STDIN" -eq 1 ]; then
  TOKEN="$(cat)"
fi

if [ -z "$TOKEN" ] || [ -z "$ACCOUNT_ID" ]; then
  echo "[secrets] error: --token (or --token-stdin) and --account-id are required" >&2
  exit 1
fi

if [ ! -f "$INDEX_JSON" ]; then
  echo "[secrets] error: index.json not found at $INDEX_JSON" >&2
  exit 1
fi

# Iterate stable, non-wip entries.
mapfile -t ENTRIES < <(
  python3 - <<PY
import json
data = json.load(open("$INDEX_JSON"))
for e in data["entries"]:
    if e.get("status") != "stable":
        continue
    if "wip" in e.get("tags", []):
        continue
    print(f"{e['name']}\t{e['url']}")
PY
)

count=0
ok=0
fail=0
for entry in "${ENTRIES[@]}"; do
  count=$((count + 1))
  if [ "$count" -gt "$LIMIT" ]; then break; fi

  name="${entry%%$'\t'*}"
  url="${entry##*$'\t'}"
  owner_repo="${url#https://github.com/}"
  owner_repo="${owner_repo%.git}"

  echo "[secrets] [$count] $name -> $owner_repo"
  if [ "$DRY_RUN" -eq 1 ]; then
    echo "[secrets]   (dry-run) would set CLOUDFLARE_API_TOKEN + CLOUDFLARE_ACCOUNT_ID"
    continue
  fi

  if ! gh repo view "$owner_repo" >/dev/null 2>&1; then
    echo "[secrets]   repo not accessible (deleted or private?), skipping"
    fail=$((fail + 1))
    continue
  fi

  # NB: must use `--body "<string>"`, NOT `--body -` with piped stdin.
  # gh CLI 2.88.1 (and probably nearby versions) silently truncates the
  # stdin form to the first character — empirically confirmed by setting
  # a 32-char known string and reading `${#SECRET}` inside an Actions run.
  if gh secret set CLOUDFLARE_API_TOKEN  --repo "$owner_repo" --body "$TOKEN"      >/dev/null 2>&1 \
     && gh secret set CLOUDFLARE_ACCOUNT_ID --repo "$owner_repo" --body "$ACCOUNT_ID" >/dev/null 2>&1; then
    echo "[secrets]   ok"
    ok=$((ok + 1))
  else
    echo "[secrets]   secret set failed (auth scope? repo permissions?)"
    fail=$((fail + 1))
  fi
done

echo
echo "[secrets] done — $ok ok, $fail fail, $count total"
