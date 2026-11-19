#!/usr/bin/env bash
# Malicious-PR suite for research challenges (design spec §3, §5): one attack
# per case, against coin-select.
#
#   scripts/malicious-prs-research.sh --local [case...]   cb gate + cb comment on local branches: no push, no PR, no minutes
#   scripts/malicious-prs-research.sh case...             live: opens the PR, waits for the verdict comment, closes it
#
# BASE (env) is the commit the cases are built on; default origin/main.
# --local always builds a scratch base on top of it with coin-select open and an
# accepted baseline row, so it works before the challenge launches. Live mode uses
# the base as is: coin-select must be open on main.
#
# WARNING: a live research PR that passes the gate and measures valid AUTO-LANDS
# on main (spec §6). Only `read-seed` and `persist-state` can do that here, so
# they must be run live only against a disposable test challenge, and need
# --yes-land. The other live
# cases are refused at the gate or rejected at measurement. Live cases are billed
# (a few minutes each), so they must be named. Needs a clean tracked tree; live
# mode needs gh auth with write access.
set -euo pipefail
# The cases check out other commits, which would swap this file under the
# running shell: run from a copy.
if [ -z "${CB_SUITE_COPY:-}" ]; then
  copy=$(mktemp) && cp "$0" "$copy"
  CB_SUITE_COPY=$copy exec bash "$copy" "$@"
fi
REPO=${REPO:-Ryun1/auto-research-challenge-builder}
C=challenges/coin-select
SUBS=$C/submissions
export GIT_AUTHOR_NAME=suite GIT_AUTHOR_EMAIL=suite@invalid GIT_COMMITTER_NAME=suite GIT_COMMITTER_EMAIL=suite@invalid

# name | expected verdict | substring
#   refused:  the gate refuses, and one of its reasons contains the substring
#   rejected: the gate passes, measurement rejects (the comment's Status line) (live only)
#   valid:    measurement is valid, so the PR would auto-land (live only, --yes-land)
#             (read-seed: the seed is hidden; persist-state: no state survives between
#             invocations: /run/lock, /var/crash, /dev/shm and SysV shm all start empty)
#   ignored:  a PR into a branch other than main starts no Submission run and gets no comment (live only)
CASES=(
  "edit-other-submission|refused|may only add files under"
  "edit-judge|refused|may only add files under"
  "unknown-parent|refused|not accepted on main"
  "deferred-contract|refused|only open contracts take submissions"
  "measure-json|refused|may only add files under"
  "symlink|refused|symlink in a submission"
  "gitlink|refused|gitlink (submodule) in a submission"
  "size-cap|refused|max bytes 4194304"
  "forged-id|refused|doesn't match directory"
  "direction-taken|refused|is already taken by accepted submission"
  "delete-submission-file|refused|may only add files under"
  "non-main-base|ignored|"
  "network-in-build|rejected|build:"
  "read-seed|valid|"
  "persist-state|valid|"
)

# A valid submission copied from the baseline: dir, id and direction are $1.
helper() {
  local d=$SUBS/$1
  cp -R "$SUBS/baseline" "$d"
  sed -E -e "s/^id( *)= .*/id\1= \"$1\"/" -e "s/^direction( *)= .*/direction\1= \"$1\"/" \
    "$SUBS/baseline/submission.toml" > "$d/submission.toml"
  git add -- "$d"
}
# Rewrites one key of a helper's submission.toml.
setkey() { # dir key value
  sed -E "s/^$2( *)= .*/$2\1= $3/" "$SUBS/$1/submission.toml" > "$SUBS/$1/submission.toml.new"
  mv "$SUBS/$1/submission.toml.new" "$SUBS/$1/submission.toml"
  git add -- "$SUBS/$1"
}

# Applies and stages one case's change. Only the paths a case touches are staged.
apply() {
  case "$1" in
    edit-other-submission) helper "$1"; echo '# pwn' >> "$SUBS/baseline/select"; git add -u -- "$SUBS/baseline" ;;
    edit-judge) helper "$1"; echo '# pwn' >> "$C/contracts/wallet/judge"; git add -u -- "$C/contracts" ;;
    unknown-parent)
      helper "$1"; setkey "$1" novelty '"builds-on"'; setkey "$1" direction '""'
      echo 'parent = "nope"' >> "$SUBS/$1/submission.toml"; git add -- "$SUBS/$1" ;;
    deferred-contract) helper "$1"; setkey "$1" contract '"wallet-multiasset@0"' ;;
    measure-json) helper "$1"; echo '{"status":"valid"}' > "$C/measure.json"; git add -- "$C/measure.json" ;;
    symlink) helper "$1"; ln -s ../baseline/select "$SUBS/$1/link"; git add -- "$SUBS/$1" ;;
    gitlink)
      # A nested repo, so `git add` records a gitlink (cacheinfo entries are dropped by add -A).
      helper "$1"; git init -q "$SUBS/$1/sub"; git -C "$SUBS/$1/sub" commit -q --allow-empty -m x
      git add -- "$SUBS/$1" 2>/dev/null ;;
    size-cap) helper "$1"; head -c 4300000 /dev/zero > "$SUBS/$1/big"; git add -- "$SUBS/$1" ;;
    forged-id) helper a; setkey a id '"b"' ;;
    direction-taken) helper "$1"; setkey "$1" direction '"largest-first"' ;;
    delete-submission-file) helper "$1"; git rm -q -- "$SUBS/baseline/README.md" ;;
    # A change the gate refuses: if the pipeline does pick the PR up, it stops there.
    non-main-base) echo '# pwn' >> "$C/contracts/wallet/judge"; git add -u -- "$C/contracts" ;;
    network-in-build)
      helper "$1"
      printf '#!/usr/bin/env bash\nset -eu\nexec 3<>/dev/tcp/1.1.1.1/443\ntest -x select\n' > "$SUBS/$1/build.sh"
      git add -- "$SUBS/$1" ;;
    read-seed)
      # The baseline, except that it fails if it can see a seed: in the environment, or as any file under /tmp.
      helper "$1"
      awk '{ print } /^import sys$/ && !d { d = 1
        print "import os"
        print "def _leak():"
        print "    if any(\"SEED\" in k.upper() for k in os.environ):"
        print "        return True"
        print "    return any(fs for _, _, fs in os.walk(\"/tmp\"))"
        print "if _leak():"
        print "    sys.exit(1)" }' "$SUBS/baseline/select" > "$SUBS/$1/select"
      chmod 755 "$SUBS/$1/select"; git add -- "$SUBS/$1" ;;
    persist-state)
      # The baseline, except that on every start it looks for state left by an earlier
      # invocation (files in /run/lock, /var/crash, /dev/shm, a SysV segment) and then
      # leaves some. If it ever FINDS state it answers the first pay with an invalid
      # tx, so the PR would be rejected. A sandbox that isolates these is valid.
      helper "$1"
      awk '{ print } /^import sys$/ && !d { d = 1
        print "import os"
        print "def _persist():"
        print "    found = False"
        print "    for d in (\"/run/lock\", \"/var/crash\", \"/dev/shm\"):"
        print "        f = os.path.join(d, \"cb-persist-state\")"
        print "        try:"
        print "            found = found or os.path.exists(f)"
        print "            open(f, \"w\").write(\"x\")"
        print "        except OSError:"
        print "            pass"
        print "    try:"
        print "        import ctypes"
        print "        libc = ctypes.CDLL(None, use_errno=True)"
        print "        key = 0x43425354"
        print "        found = found or libc.shmget(key, 0, 0) >= 0"
        print "        libc.shmget(key, 4096, 0o1000 | 0o666)"
        print "    except Exception:"
        print "        pass"
        print "    return found"
        print "if _persist():"
        print "    sys.stdout.write(\"{\\\"inputs\\\":[],\\\"change\\\":[],\\\"fee\\\":0}\\n\")"
        print "    sys.stdout.flush()"
        print "    sys.exit(0)" }' "$SUBS/baseline/select" > "$SUBS/$1/select"
      chmod 755 "$SUBS/$1/select"; git add -- "$SUBS/$1" ;;
    *) echo "no change defined for case $1" >&2; return 1 ;;
  esac
}

local_mode=false yes_land=false
want=()
for a in "$@"; do
  case "$a" in
    --local) local_mode=true ;;
    --yes-land) yes_land=true ;;
    *) want+=("$a") ;;
  esac
done
for w in ${want[@]+"${want[@]}"}; do
  printf '%s\n' "${CASES[@]}" | grep -q "^$w|" || { echo "unknown case: $w" >&2; exit 2; }
done
if ! $local_mode && [ ${#want[@]} -eq 0 ]; then
  echo "live runs are billed: name the cases to run, or use --local" >&2
  printf 'cases: %s\n' "$(printf '%s\n' "${CASES[@]}" | cut -d'|' -f1 | tr '\n' ' ')" >&2
  exit 2
fi
if ! $local_mode && [[ " ${want[*]} " == *" read-seed "* || " ${want[*]} " == *" persist-state "* ]]; then
  echo "WARNING: read-seed and persist-state are valid submissions: live, it AUTO-LANDS on main if the sandbox hides the seed." >&2
  echo "Run it only against a disposable test challenge (REPO and BASE pointing at it)." >&2
  $yes_land || { echo "refusing: pass --yes-land to run read-seed or persist-state live" >&2; exit 2; }
fi
git diff --quiet && git diff --cached --quiet || { echo "tracked changes in the working tree: commit or stash them first" >&2; exit 2; }

start=$(git symbolic-ref --short -q HEAD || git rev-parse HEAD)
base=$(git rev-parse "${BASE:-origin/main}")
tmp=$(mktemp -d)
branch="" basebranch="" pr=""

# Runs after every case and on exit: nothing a case made outlives it.
cleanup_case() {
  if [ -n "$pr" ]; then gh pr close "$pr" --repo "$REPO" >/dev/null 2>&1 || true; fi
  if [ -n "$branch" ] && ! $local_mode; then
    # A run still going on the case's branch is billed until it stops.
    for id in $(gh run list --repo "$REPO" --branch "$branch" --json databaseId,status \
      --jq '.[] | select(.status != "completed") | .databaseId' 2>/dev/null || true); do
      gh run cancel "$id" --repo "$REPO" >/dev/null 2>&1 || true
    done
    git push -q origin --delete "$branch" 2>/dev/null || true
  fi
  if [ -n "$basebranch" ]; then git push -q origin --delete "$basebranch" 2>/dev/null || true; fi
  git checkout -q -f --detach "$base" 2>/dev/null || true
  # Untracked leftovers of a case (a nested repo needs -ff).
  git clean -ffdq -- "$C" 2>/dev/null || true
  if [ -n "$branch" ]; then git branch -qD "$branch" 2>/dev/null || true; fi
  branch="" basebranch="" pr=""
}
finish() { cleanup_case; git checkout -q "$start" 2>/dev/null || true; rm -rf "$tmp" "$CB_SUITE_COPY"; }
trap finish EXIT

if $local_mode; then
  cargo build --release -q
  # A scratch base: coin-select open, with an accepted baseline on the map.
  git checkout -q --detach "$base"
  # Only the first status line: the challenge's, not a contract's.
  awk '!d && /^status *= *"/ { sub(/"[a-z]+"/, "\"open\""); d = 1 } { print }' "$C/challenge.toml" > "$tmp/challenge.toml"
  cat "$tmp/challenge.toml" > "$C/challenge.toml"
  if [ -s "$C/results.jsonl" ] && [ -n "$(tail -c1 "$C/results.jsonl")" ]; then echo >> "$C/results.jsonl"; fi
  echo '{"schema":2,"type":"research","stage":"baseline","status":"accepted","submission":"baseline","direction":"largest-first","slug":"coin-select"}' >> "$C/results.jsonl"
  git add -- "$C/challenge.toml" "$C/results.jsonl"
  git commit -qm "suite: scratch base (coin-select open, accepted baseline)"
  base=$(git rev-parse HEAD)
fi

verdict_comment() {
  gh pr view "$pr" --repo "$REPO" --json comments \
    --jq '[.comments[].body | select(contains("cb:score"))] | last // ""' 2>/dev/null || true
}

# The verdict AND the reason must match: a crashed gate is not a refusal.
# cb comment neutralises the reasons (<>&`|[]@*_# become spaces), so the
# substrings in CASES are ones that survive it (max_bytes shows as "max bytes").
refusal_matches() {
  grep -qF "Refused by the gate" <<<"$1" && grep -qF "$2" <<<"$1" && ! grep -q "failed before deciding" <<<"$1"
}
# A measured verdict: the gate passed and the comment's Status line says $2.
status_matches() {
  ! grep -qF "Refused by the gate" <<<"$1" && grep -qE "Status: $2( |$)" <<<"$1"
}

fail=0
for c in "${CASES[@]}"; do
  IFS='|' read -r name expect why <<<"$c"
  if [ ${#want[@]} -gt 0 ] && [[ ! " ${want[*]} " == *" $name "* ]]; then continue; fi
  if $local_mode && [ "$expect" != refused ]; then
    printf '%-24s %-8s SKIP (live only)\n' "$name" "$expect"; continue
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
      --body "research suite: a PR into a branch other than main; expect no Submission run and no comment" | grep -o '[0-9]*$')
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
      --body "research suite; expect $expect" | grep -o '[0-9]*$')
    body=""
    # Refusals comment at once; a measured verdict takes the whole build and measure run.
    for _ in $(seq 1 120); do
      body=$(verdict_comment)
      if [ -n "$body" ] && { [ "$expect" = refused ] || grep -qE "Refused by the gate|Status: " <<<"$body"; }; then break; fi
      sleep 10
    done
    if [ "$expect" = refused ]; then
      refusal_matches "$body" "$why" && ok=1
    else
      status_matches "$body" "$expect" && ok=1
      if [ "$expect" = rejected ] && [ $ok = 1 ] && [ -n "$why" ]; then grep -qF "$why" <<<"$body" || ok=0; fi
    fi
    note=$(grep -m1 -E 'Status: |^- ' <<<"$body" | cut -c1-110 || true)
    [ -n "$body" ] || note="no verdict comment in 20 min"
  fi
  printf '%-24s %-8s %s%s  %s\n' "$name" "$expect" "$([ $ok = 1 ] && echo PASS || echo FAIL)" "${pr:+  PR #$pr}" "$note"
  [ $ok = 1 ] || fail=1
  cleanup_case
done
exit $fail
