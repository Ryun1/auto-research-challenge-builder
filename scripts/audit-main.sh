#!/usr/bin/env bash
# Tamper evidence for main (#22). The ruleset in .github/rulesets/main.json
# is what prevents; this checks that nothing got past it, and that the two
# bypasses (the owner, and the deploy key the workflows push results with)
# were only used as intended.
#
# It reads GitHub's push log for main (the Activity API, which a collaborator
# cannot rewrite) and reports every update that is not:
#   - a PR merged by the repo owner, or
#   - a results/revert commit pushed by the Comment, Promote or Accept workflow
#     as it runs from main: the push must fall inside such a run and touch only
#     results.jsonl, a challenge's candidate/ or a research challenge's
#     submissions/<id>/ (landed by Comment, removed by Accept). Deploy-key pushes are logged
#     under the account that added the key (the owner); before the ruleset
#     they were logged as github-actions[bot].
# Run it from your own clone before merging:
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
# (workflow_run), and Promote and Accept count when it ran on main from a merge or from a
# dispatch by the owner.
gh api --paginate "repos/$repo/actions/runs?per_page=100&created=>=${since%T*}" --jq '.workflow_runs[]' \
  | jq -s --arg owner "$owner" 'map(select(
        (.path == ".github/workflows/comment.yml" and .event == "workflow_run")
        or ((.path == ".github/workflows/promote.yml" or .path == ".github/workflows/accept.yml") and .head_branch == "main"
            and (.event == "push" or (.event == "workflow_dispatch" and .triggering_actor.login == $owner))))
      | {name, id, url: .html_url, from: (.run_started_at | fromdate), to: (.updated_at | fromdate)})' \
  > "$tmp/runs.json"

findings=0
flag() { findings=$((findings + 1)); echo "FLAG  $*"; }

while IFS=$'\t' read -r at kind actor before after; do
  short=${after:0:7}
  case "$kind:$actor" in
    pr_merge:"$owner") ;;
    push:"github-actions[bot]" | push:"$owner")
      run=$(jq -r --arg at "$at" '($at | fromdate) as $t
        | map(select(.from - 5 <= $t and $t <= .to + 60)) | (.[0].name // empty)' "$tmp/runs.json")
      files=$(gh api "repos/$repo/compare/$before...$after" --jq '.files[].filename')
      odd=$(printf '%s\n' "$files" | grep -Ev '^challenges/[^/]+/(results\.jsonl|candidate/.+|submissions/[^/]+/.+)$' || true)
      if [ -n "$run" ] && [ -z "$odd" ]; then
        : # a results commit from Comment, Promote or Accept
      elif [ "$actor" = "$owner" ]; then
        echo "note  $at $short pushed to main directly as $owner, not through a PR and not a workflow's results commit"
      else
        [ -n "$run" ] || flag "$at $short pushed as $actor outside any Comment, Promote or Accept run on main"
        [ -z "$odd" ] || flag "$at $short push as $actor changed files other than results.jsonl, candidate/ or submissions/: $(echo $odd)"
      fi
      ;;
    *) flag "$at $short $kind by $actor" ;;
  esac
done < <(jq -r '.[] | [.timestamp, .activity_type, (.actor.login // "unknown"), .before, .after] | @tsv' "$tmp/activity.json")

total=$(jq length "$tmp/activity.json")
if [ "$findings" -eq 0 ]; then
  echo "ok: $total updates to main since $since, none flagged (notes above, if any, are the owner's own direct pushes)"
else
  echo "$findings finding(s) in $total updates to main since $since"
  exit 1
fi
