#!/usr/bin/env bash
# Malicious-PR suite (#13, #22): one attack per case, against ledger-cbor.
#
#   scripts/malicious-prs.sh --local [case...]   cb gate on local branches: no push, no PR, no minutes
#   scripts/malicious-prs.sh case...             live: opens the PR, waits for the verdict, closes it
#
# Live cases are billed (a few minutes each), so they must be named. Every case
# here stops at the gate: none reaches the score job. Needs a clean tracked
# tree; live mode needs gh auth with write access.
set -euo pipefail
# The cases check out other commits, which would swap this file under the
# running shell: run from a copy.
if [ -z "${CB_SUITE_COPY:-}" ]; then
  copy=$(mktemp) && cp "$0" "$copy"
  CB_SUITE_COPY=$copy exec bash "$copy" "$@"
fi
REPO=${REPO:-Ryun1/auto-research-challenge-builder}
L=challenges/ledger-cbor
S=$L/candidate/cardano-ledger-binary/src/Cardano/Ledger/Binary/Decoding/Sized.hs

# name | expected verdict | reason substring
#   refused: the gate refuses, and one of its reasons contains the substring
#   ignored: a PR into a branch other than main starts no Submission run and gets no comment (live only)
CASES=(
  "workflow-edit|refused|exactly one challenge"
  "score-json|refused|score.json: outside the editable paths"
  "symlink|refused|symlink"
  "gitlink|refused|gitlink"
  "verifier-edit|refused|judge.py: outside the editable paths"
  "setup-edit|refused|setup.sh: outside the editable paths"
  "size-cap|refused|editable tree is"
  "new-file|refused|new file (the file set is frozen)"
  "th-extension|refused|extension TemplateHaskell is not allowed"
  "unsafe-import|refused|import of System.IO.Unsafe"
  "foreign-decl|refused|foreign declaration"
  "non-main-base|ignored|"
)

# Applies and stages one case's change. Only the paths a case touches are
# staged: `git add -A` would also commit whatever is untracked in this checkout.
th_pragma() { { echo '{-# LANGUAGE TemplateHaskell #-}'; cat "$S"; } > "$S.new" && mv "$S.new" "$S"; }
apply() {
  case "$1" in
    workflow-edit) echo '# pwn' >> .github/workflows/ci.yml; echo '-- pwn' >> "$S" ;;
    score-json) echo '{"metrics": {"speedup": 9}}' > "$L/score.json"; git add -- "$L/score.json" ;;
    symlink) ln -s ../verifier/judge.py "$L/candidate/link"; git add -- "$L/candidate/link" ;;
    gitlink) git update-index --add --cacheinfo "160000,$(git rev-parse HEAD),$L/candidate/sub" ;;
    verifier-edit) echo '# pwn' >> "$L/verifier/judge.py" ;;
    setup-edit) echo '# pwn' >> "$L/setup.sh" ;;
    size-cap) { printf '\n-- '; head -c 2000000 /dev/zero | tr '\0' x; echo; } >> "$S" ;;
    new-file)
      echo 'module Cardano.Ledger.Binary.Decoding.Pwn where' > "$(dirname "$S")/Pwn.hs"
      git add -- "$(dirname "$S")/Pwn.hs" ;;
    th-extension) th_pragma ;;
    # Before the first import: the last one may span several lines.
    unsafe-import)
      awk '!done && /^import / { print "import System.IO.Unsafe (unsafePerformIO)"; done = 1 } { print }' "$S" > "$S.new"
      mv "$S.new" "$S" ;;
    foreign-decl) echo 'foreign import ccall "getenv" c_getenv :: Int -> Int' >> "$S" ;;
    # A candidate change the gate refuses: if the pipeline does pick the PR up,
    # it stops at the gate, and a candidate-only PR starts no CI run.
    non-main-base) th_pragma ;;
    *) echo "no change defined for case $1" >&2; return 1 ;;
  esac
  # A gitlink lives only in the index: add -u would drop it again.
  [ "$1" = gitlink ] || git add -u
}

local_mode=false
want=()
for a in "$@"; do
  if [ "$a" = --local ]; then local_mode=true; else want+=("$a"); fi
done
for w in ${want[@]+"${want[@]}"}; do
  printf '%s\n' "${CASES[@]}" | grep -q "^$w|" || { echo "unknown case: $w" >&2; exit 2; }
done
if ! $local_mode && [ ${#want[@]} -eq 0 ]; then
  echo "live runs are billed: name the cases to run, or use --local" >&2
  printf 'cases: %s\n' "$(printf '%s\n' "${CASES[@]}" | cut -d'|' -f1 | tr '\n' ' ')" >&2
  exit 2
fi
git diff --quiet && git diff --cached --quiet || { echo "tracked changes in the working tree: commit or stash them first" >&2; exit 2; }

start=$(git symbolic-ref --short -q HEAD || git rev-parse HEAD)
base=$(git rev-parse origin/main)
tmp=$(mktemp -d)
branch="" basebranch="" pr=""

# Runs after every case and on exit: nothing a case made outlives it.
cleanup_case() {
  if [ -n "$pr" ]; then gh pr close "$pr" --repo "$REPO" >/dev/null 2>&1 || true; fi
  if [ -n "$branch" ] && ! $local_mode; then
    # A run still going on the case's branch (CI on a judge-touching case) is billed until it stops.
    for id in $(gh run list --repo "$REPO" --branch "$branch" --json databaseId,status \
      --jq '.[] | select(.status != "completed") | .databaseId' 2>/dev/null || true); do
      gh run cancel "$id" --repo "$REPO" >/dev/null 2>&1 || true
    done
    git push -q origin --delete "$branch" 2>/dev/null || true
  fi
  if [ -n "$basebranch" ]; then git push -q origin --delete "$basebranch" 2>/dev/null || true; fi
  git checkout -q -f --detach "$base" 2>/dev/null || true
  if [ -n "$branch" ]; then git branch -qD "$branch" 2>/dev/null || true; fi
  branch="" basebranch="" pr=""
}
finish() { cleanup_case; git checkout -q "$start" 2>/dev/null || true; rm -rf "$tmp" "$CB_SUITE_COPY"; }
trap finish EXIT

if $local_mode; then cargo build --release -q; fi

verdict_comment() {
  gh pr view "$pr" --repo "$REPO" --json comments \
    --jq '[.comments[].body | select(contains("cb:score"))] | last // ""' 2>/dev/null || true
}

# The verdict AND the reason must match: a crashed gate is not a refusal.
# cb comment neutralises the reasons (">" and "_" become spaces), so the
# substrings in CASES are ones that survive it.
refusal_matches() {
  grep -qF "Refused by the gate" <<<"$1" && grep -qF "$2" <<<"$1" && ! grep -q "failed before deciding" <<<"$1"
}

fail=0
for c in "${CASES[@]}"; do
  IFS='|' read -r name expect why <<<"$c"
  if [ ${#want[@]} -gt 0 ] && [[ ! " ${want[*]} " == *" $name "* ]]; then continue; fi
  if $local_mode && [ "$expect" = ignored ]; then
    printf '%-16s %-8s SKIP (live only)\n' "$name" "$expect"; continue
  fi
  branch="malicious/$name-$(date +%s)"
  git checkout -q --detach "$base"
  git checkout -q -b "$branch"
  apply "$name"
  git commit -qm "malicious: $name"
  ok=0 note=""
  if $local_mode; then
    ./target/release/cb gate --base "$base" --head HEAD --out "$tmp/sub-$name" --json "$tmp/gate-$name.json" >/dev/null 2>&1 || true
    # Matched on the comment the PR would get, exactly as a live run matches.
    body=$(./target/release/cb comment --gate "$tmp/gate-$name.json")
    if [ "$(jq -r .ok "$tmp/gate-$name.json")" = false ] && refusal_matches "$body" "$why"; then ok=1; fi
    note=$( (grep -F -- "$why" <<<"$body" || grep -E '^- ' <<<"$body" || echo "gate ok: not refused") | head -1 | cut -c1-110)
  elif [ "$expect" = ignored ]; then
    basebranch="malicious/base-$(date +%s)"
    git push -q origin "$base:refs/heads/$basebranch" "$branch"
    pr=$(gh pr create --repo "$REPO" --base "$basebranch" --head "$branch" --title "malicious: $name" \
      --body "#22 suite: a PR into a branch other than main; expect no Submission run and no comment" | grep -o '[0-9]*$')
    ok=1
    for _ in $(seq 1 12); do
      sleep 10
      runs=$(gh run list --repo "$REPO" --workflow Submission --branch "$branch" --json databaseId --jq length)
      if [ "$runs" != 0 ]; then ok=0; note="a Submission run started for a PR into $basebranch"; break; fi
      if [ -n "$(verdict_comment)" ]; then ok=0; note="the pipeline commented on a PR into $basebranch"; break; fi
    done
    [ $ok = 0 ] || note="no Submission run and no comment in 120 s"
  else
    git push -q origin "$branch"
    pr=$(gh pr create --repo "$REPO" --base main --head "$branch" --title "malicious: $name" \
      --body "#22 suite; expect $expect" | grep -o '[0-9]*$')
    body=""
    for _ in $(seq 1 90); do
      body=$(verdict_comment)
      [ -n "$body" ] && break
      sleep 10
    done
    if refusal_matches "$body" "$why"; then ok=1; fi
    note=$(grep -m1 -E '^- ' <<<"$body" | cut -c1-110 || true)
    [ -n "$body" ] || note="no verdict comment in 15 min"
  fi
  printf '%-16s %-8s %s%s  %s\n' "$name" "$expect" "$([ $ok = 1 ] && echo PASS || echo FAIL)" "${pr:+  PR #$pr}" "$note"
  [ $ok = 1 ] || fail=1
  cleanup_case
done
exit $fail
