#!/usr/bin/env bash
# Creates (or rotates) the two deploy keys the workflows push with, and stores
# each private half as an environment secret. Run by the repo owner; needs gh
# with admin rights on both repos. See docs/pipeline.md, "Protection of main".
#
#   results  write key on this repo      -> secret RESULTS_DEPLOY_KEY, environment `results`
#   site     write key on the site repo  -> secret SITE_DEPLOY_KEY,    environment `site`
#
# Both environments must already exist and be limited to the main branch
# (Settings > Environments), so only runs on main can read the secrets.
# The private keys exist only in a temp dir that is removed on exit.
# Old keys are left in place: delete them under Settings > Deploy keys once
# a run has pushed with the new ones.
set -euo pipefail
repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
site=${SITE_REPO:-${repo%%/*}/auto-research-challenges}
stamp=$(date -u +%F)

for env in results site; do
  policy=$(gh api "repos/$repo/environments/$env/deployment-branch-policies" --jq '[.branch_policies[].name] | join(",")')
  [ "$policy" = main ] || { echo "environment $env must be limited to main (found: ${policy:-none})" >&2; exit 1; }
done

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
ssh-keygen -q -t ed25519 -N "" -C "cb results commits $stamp" -f "$tmp/results"
ssh-keygen -q -t ed25519 -N "" -C "cb site deploy $stamp" -f "$tmp/site"

gh repo deploy-key add "$tmp/results.pub" -R "$repo" --allow-write \
  --title "results commits: Comment and Promote (environment results, $stamp)"
gh secret set RESULTS_DEPLOY_KEY -R "$repo" --env results < "$tmp/results"

gh repo deploy-key add "$tmp/site.pub" -R "$site" --allow-write \
  --title "Site workflow (environment site, $stamp)"
gh secret set SITE_DEPLOY_KEY -R "$repo" --env site < "$tmp/site"

echo "done: RESULTS_DEPLOY_KEY (environment results) and SITE_DEPLOY_KEY (environment site) are set"
