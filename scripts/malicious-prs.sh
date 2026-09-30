#!/usr/bin/env bash
# Malicious-PR suite (#13): opens one PR per attack against the live
# pipeline, waits for its verdict comment, checks it, closes the PR.
# Needs gh auth with write access. Usage: scripts/malicious-prs.sh [case...]
set -euo pipefail
REPO=${REPO:-Ryun1/auto-research-challenge-builder}
G=challenges/golomb-14
P=challenges/pedersen

# name | expected verdict (refused | rejected) | reason substring | change applied on a fresh branch
declare -a CASES=(
  "workflow-edit|refused|exactly one challenge|echo '# pwn' >> .github/workflows/ci.yml && echo '0 4 6 20 35 52 59 77 78 86 89 99 122 127 ' > $G/candidate/ruler.txt"
  "score-json|refused|score.json: outside the editable paths|echo '{\"metrics\": {\"length\": 1}}' > $G/score.json"
  "symlink|refused|link: symlink|ln -s ../verifier/golomb.py $G/candidate/link"
  "gitlink|refused|gitlink|git update-index --add --cacheinfo 160000,\$(git rev-parse HEAD),$G/candidate/sub"
  "verifier-edit|refused|golomb.py: outside the editable paths|echo 'print(\"{\\\"ok\\\": true}\")' > $G/verifier/golomb.py"
  "lockfile-edit|refused|Cargo.lock: outside the editable paths|echo '# pwn' >> $P/Cargo.lock"
  "size-cap|refused|max bytes|head -c 5000 /dev/zero | tr '\\\\0' 7 > $G/candidate/ruler.txt"
  "read-seed-and-reference|refused|not allowed: environment|printf '\\npub fn leak() -> String { std::env::var(\"CB_SEED\").unwrap_or_default() + &std::fs::read_to_string(\"../reference/src/lib.rs\").unwrap_or_default() }\\n' >> $P/candidate/src/lib.rs"
  "wrong-answer|rejected|verify failed|sed -i.bak 's/    acc.to_affine().x().to_bytes_be()/    let mut o = acc.to_affine().x().to_bytes_be(); o[0] ^= 1; o/' $P/candidate/src/lib.rs && rm $P/candidate/src/lib.rs.bak && grep -qF 'o[0] ^= 1' $P/candidate/src/lib.rs"
)

want=("$@")
fail=0
base=$(git rev-parse origin/main)
for c in "${CASES[@]}"; do
  IFS='|' read -r name expect why change <<<"$c"
  if [ ${#want[@]} -gt 0 ] && [[ ! " ${want[*]} " == *" $name "* ]]; then continue; fi
  branch="malicious/$name-$(date +%s)"
  git checkout -q --detach "$base"
  git checkout -q -b "$branch"
  bash -c "$change"
  # A gitlink lives only in the index; add -A would drop it.
  [ "$name" = gitlink ] || git add -A
  git commit -qm "malicious: $name" && git push -q origin "$branch" 2>/dev/null
  pr=$(gh pr create --repo "$REPO" --base main --head "$branch" --title "malicious: $name" --body "#13 suite; expect $expect" | grep -o '[0-9]*$')
  body=""
  for _ in $(seq 1 90); do
    body=$(gh pr view "$pr" --repo "$REPO" --json comments --jq '[.comments[].body | select(contains("cb:score"))] | last // ""' 2>/dev/null || true)
    [ -n "$body" ] && break
    sleep 10
  done
  # The verdict AND the reason must match: a crashed gate is not a refusal.
  case "$expect" in
    refused) marker="Refused by the gate" ;;
    rejected) marker="| status | rejected |" ;;
  esac
  ok=0
  if grep -qF "$marker" <<<"$body" && grep -qF "$why" <<<"$body" && ! grep -q "failed before deciding" <<<"$body"; then ok=1; fi
  reason=$(grep -m1 -E '^- |Reason:' <<<"$body" | cut -c1-110 || true)
  printf '%-26s %-9s %s  PR #%s  %s\n' "$name" "$expect" "$([ $ok = 1 ] && echo PASS || echo FAIL)" "$pr" "$reason"
  [ $ok = 1 ] || fail=1
  gh pr close "$pr" --repo "$REPO" --delete-branch >/dev/null 2>&1 || true
  git checkout -q --detach "$base"; git branch -qD "$branch"
done
git checkout -q main
exit $fail
