#!/usr/bin/env bash
# Daily cap on Submission runs per actor (docs/pipeline.md, "Actions minutes"):
# every run is billed. Counts the actor's Submission runs created in the last
# 24 hours, the asking run included. Above CAP it writes gate.json in the
# current directory as a gate refusal (Comment posts it) and prints
# capped=true; otherwise it writes nothing and prints capped=false.
# A failed API call exits non-zero, so the gate job fails closed.
# Env: REPO (owner/name), ACTOR (login), CAP (integer), GH_TOKEN.
set -euo pipefail
: "${REPO:?}" "${ACTOR:?}" "${CAP:?}"
[[ "$CAP" =~ ^[0-9]+$ ]] || { echo "CAP must be an integer, got '$CAP'" >&2; exit 2; }
since=$(date -u -d '24 hours ago' +%FT%TZ 2>/dev/null || date -u -v-24H +%FT%TZ) # GNU, then BSD
actor=$(jq -rn --arg a "$ACTOR" '$a | @uri')
ids=$(gh api --paginate \
  "repos/$REPO/actions/workflows/submission.yml/runs?actor=$actor&created=>=$since&per_page=100" \
  --jq '.workflow_runs[].id')
n=$(grep -c . <<<"$ids" || true)
if [ "$n" -gt "$CAP" ]; then
  jq -n --arg a "$ACTOR" --arg n "$n" --arg cap "$CAP" '{ok: false, errors: [
    "daily cap: \($a) started \($n) Submission runs in the last 24 hours (limit \($cap)). Every run is billed; push again tomorrow or ask the maintainer."]}' > gate.json
  echo "capped=true"
else
  echo "capped=false"
fi
