#!/usr/bin/env bash
# Tamper evidence for main while the repo has no branch protection (#22).
#
# Collaborators need write access to submit, and nothing stops a writer from
# pushing to main. This reads GitHub's push log for main (the Activity API,
# which a collaborator cannot rewrite) and reports every update that is not:
#   - a PR merged by the repo owner, or
#   - a results/revert commit pushed by the Comment or Promote workflow as it
#     runs from main (a workflow on another branch can also push as
#     github-actions[bot], so the push must fall inside such a run and touch
#     only results.jsonl or a challenge's candidate/).
# It detects; it does not prevent. Run it from your own clone before merging:
#   scripts/audit-main.sh [day|week|month|quarter|year]   (default: month)
# Exit 0: nothing unexpected. Exit 1: findings. Needs gh and jq.
set -euo pipefail
period=${1:-month}
# Updates up to here were reviewed by hand on 2026-11-12. Until #66 merged, the
# results commits also rewrote public/, which the file check below would flag.
reviewed_until=2026-11-11T22:39:20Z
repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
owner=${repo%%/*}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

gh api --paginate "repos/$repo/activity?ref=refs/heads/main&time_period=$period&per_page=100" \
  | jq -s --arg from "$reviewed_until" 'add // [] | map(select(.timestamp > $from))' > "$tmp/activity.json"
since=$(jq -r 'map(.timestamp) | min // empty' "$tmp/activity.json")
[ -n "$since" ] || { echo "no updates to main in the last $period (after $reviewed_until)"; exit 0; }

# Runs that are allowed to push: Comment only ever runs main's copy
# (workflow_run), and Promote counts when it ran on main from a merge or from a
# dispatch by the owner.
gh api --paginate "repos/$repo/actions/runs?per_page=100&created=>=${since%T*}" --jq '.workflow_runs[]' \
  | jq -s --arg owner "$owner" 'map(select(
        (.path == ".github/workflows/comment.yml" and .event == "workflow_run")
        or (.path == ".github/workflows/promote.yml" and .head_branch == "main"
            and (.event == "push" or (.event == "workflow_dispatch" and .triggering_actor.login == $owner))))
      | {name, id, url: .html_url, from: (.run_started_at | fromdate), to: (.updated_at | fromdate)})' \
  > "$tmp/runs.json"

findings=0
flag() { findings=$((findings + 1)); echo "FLAG  $*"; }

while IFS=$'\t' read -r at kind actor before after; do
  short=${after:0:7}
  case "$kind:$actor" in
    pr_merge:"$owner") ;;
    push:"github-actions[bot]")
      run=$(jq -r --arg at "$at" '($at | fromdate) as $t
        | map(select(.from - 5 <= $t and $t <= .to + 60)) | (.[0].name // empty)' "$tmp/runs.json")
      files=$(gh api "repos/$repo/compare/$before...$after" --jq '.files[].filename')
      odd=$(printf '%s\n' "$files" | grep -Ev '^challenges/[^/]+/(results\.jsonl|candidate/.+)$' || true)
      [ -n "$run" ] || flag "$at $short pushed as github-actions[bot] outside any Comment or Promote run on main"
      [ -z "$odd" ] || flag "$at $short bot push changed files other than results.jsonl or candidate/: $(echo $odd)"
      ;;
    push:"$owner") echo "note  $at $short pushed to main directly by $owner (not through a PR)" ;;
    *) flag "$at $short $kind by $actor" ;;
  esac
done < <(jq -r '.[] | [.timestamp, .activity_type, (.actor.login // "unknown"), .before, .after] | @tsv' "$tmp/activity.json")

total=$(jq length "$tmp/activity.json")
if [ "$findings" -eq 0 ]; then
  echo "ok: $total updates to main since $since, all PR merges by $owner or results commits from Comment/Promote"
else
  echo "$findings finding(s) in $total updates to main since $since"
  exit 1
fi
