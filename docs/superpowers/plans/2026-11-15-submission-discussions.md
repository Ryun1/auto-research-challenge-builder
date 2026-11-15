# Submission Discussions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every submission PR gets a GitHub Discussion thread that the bot keeps up to date, holding a required write-up (Idea / What worked / What didn't) taken from the PR description. A record-claiming PR without a complete write-up is flagged as not ready to merge.

**Architecture:** `cb` renders all the text and parses the PR description as untrusted data: thread title, facts block, write-up section, write-up status comment. A small shell helper, `.github/scripts/discussion.sh`, talks to GitHub: GraphQL for discussions, REST for PR comments. `comment.yml` gets one extra step that can fail without breaking anything, and a new `writeup.yml` reacts when a PR description is edited. Neither runs submission code.

**Tech Stack:** Rust (`cb` crate, clap, serde_json), bash with `gh api` / `gh api graphql`, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-11-14-submission-discussions-design.md`

**Deviation from the spec (a builder's call the spec allowed):** the not-ready flag and the thread link go in a separate bot comment, the **write-up status comment** (marker `<!-- cb:writeup thread=N -->`). They are not added to the score comment. That way `writeup.yml` can update the flag without the score artifacts, `cb comment` stays unchanged, and the status comment is also where later runs find the thread number. Hostile PR descriptions are covered by unit tests rather than a new `scripts/malicious-prs.sh` case. That suite only exercises the gate, and a live case costs Actions minutes. Task 6 runs one live hostile-description check.

## Global Constraints

- Discussion category name: exactly `Submissions`. Thread title: `[<slug>] PR #<n>: <PR title>`, or `[refused] PR #<n>: <PR title>` when the gate refused the PR.
- Required headings: `## Idea`, `## What worked`, `## What didn't`.
- A **record claim** is a PR-stage results entry with `status == "valid_claim_better"`.
- PR text is untrusted: no `@` mention may notify anyone, no raw HTML or HTML comment may survive, and it can't fake the bot's facts block or markers.
- Neither workflow ever checks out, builds or runs PR code. Both run main's copy only, and neither uses an Actions cache (on `pull_request_target` a cache could be poisoned).
- A failure in the discussion step must not lose the score comment or the results commit.
- No extra workflow run per submission. `writeup.yml` runs only when a PR description is edited.
- A discussion body stays under 65,536 characters. Each write-up section is capped at 8,000 characters, with the cut marked `…(truncated)`.

## Review Focus

1. Empty or missing PR description (`gh` returns `""` or `null`): all three sections reported missing, no panic. Test in Task 1.
2. Heading variants people actually type, such as `### What worked:`, `## what didn’t work` (curly apostrophe) or `## What did not work`, count as the heading. Test in Task 1.
3. A `## Idea` line inside a fenced code block isn't a heading, and code inside fences keeps `<`, `_` and backticks untouched. Test in Task 1.
4. A PR refused by the gate (no slug, no score) still gets a thread titled `[refused] …` and a status comment. Test in Task 2.
5. A user's text can't add a second `<!-- cb:writeup -->` marker that `refresh` would splice at. Test in Task 2.

---

### Task 1: Write-up parsing, cleanup and status comment (`writeup.rs`)

**Files:**
- Create: `crates/cb/src/writeup.rs` (tests in-module, `#[cfg(test)] mod tests`)
- Modify: `crates/cb/src/main.rs` (add `mod writeup;`)

**Interfaces:**
- Produces:
  - `pub struct Writeup { pub idea: String, pub worked: String, pub didnt: String }`. An empty string means missing; the strings are raw, not yet cleaned.
  - `pub fn parse(body: &str) -> Writeup`
  - `impl Writeup { pub fn missing(&self) -> Vec<&'static str> }` returns names, in order, from `"Idea"`, `"What worked"`, `"What didn't"`
  - `pub fn sanitize(s: &str) -> String`
  - `pub fn render(w: &Writeup) -> String`: starts with `<!-- cb:writeup -->\n`, then `### Idea`, `### What worked`, `### What didn't`, each with its cleaned text or `_Missing._`
  - `pub fn status_comment(w: &Writeup, claim: Option<&str>, repo: &str, thread: Option<u64>) -> String`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn parses_three_sections() {
    let w = parse("intro\n## Idea\nskip Coders\n## What worked\n-3% Ir\n## What didn't\ninlining\n");
    assert_eq!((w.idea.trim(), w.worked.trim(), w.didnt.trim()), ("skip Coders", "-3% Ir", "inlining"));
    assert!(w.missing().is_empty());
}
#[test] fn empty_body_is_all_missing() {
    assert_eq!(parse("").missing(), vec!["Idea", "What worked", "What didn't"]);
}
#[test] fn heading_variants() {
    let w = parse("### idea:\na\n## What worked:\nb\n## what didn’t work\nc\n");
    assert!(w.missing().is_empty());
    let w = parse("## Idea\na\n## What worked\nb\n## What did not work\nc\n");
    assert!(w.missing().is_empty());
}
#[test] fn template_placeholders_count_as_missing() {
    let w = parse("## Idea\n<!-- one or two sentences -->\n## What worked\n  \n## What didn't\nx\n");
    assert_eq!(w.missing(), vec!["Idea", "What worked"]);
}
#[test] fn headings_in_code_fences_are_ignored() {
    let w = parse("## Idea\n```\n## What worked\n```\n");
    assert_eq!(w.missing(), vec!["What worked", "What didn't"]);
    assert!(w.idea.contains("## What worked"));
}
#[test] fn sanitize_defuses_mentions_html_and_markers() {
    let s = sanitize("ping @everyone <img src=x onerror=1> <!-- cb:writeup --> `Vec<u8>` decode_multiasset\n```\nlet x: Vec<u8>;\n```");
    assert!(!s.contains("@everyone") && !s.contains("<img") && !s.contains("<!--"));
    assert!(s.contains("`Vec<u8>`") && s.contains("let x: Vec<u8>;") && s.contains("decode_multiasset"));
}
#[test] fn sanitize_demotes_headings_and_caps_length() {
    let s = sanitize(&format!("# Score: 9x\n{}", "a".repeat(9000)));
    assert!(s.starts_with("#### Score: 9x"));
    assert!(s.chars().count() <= 8000 + 20 && s.ends_with("…(truncated)"));
}
#[test] fn status_flags_record_claims_only() {
    let w = parse("## Idea\na\n");
    let c = status_comment(&w, Some("valid_claim_better"), "o/r", Some(7));
    assert!(c.starts_with("<!-- cb:writeup thread=7 -->"));
    assert!(c.contains("Not ready to merge") && c.contains("What worked, What didn't"));
    assert!(c.contains("https://github.com/o/r/discussions/7"));
    let c = status_comment(&w, Some("valid_not_promoted"), "o/r", None);
    assert!(c.starts_with("<!-- cb:writeup -->") && !c.contains("Not ready") && c.contains("Write-up missing"));
    let full = parse("## Idea\na\n## What worked\nb\n## What didn't\nc\n");
    assert!(status_comment(&full, Some("valid_claim_better"), "o/r", Some(7)).contains("Write-up complete"));
}
```

- [ ] **Step 2: Run the tests and check they fail**

Run: `cargo test -p cb writeup`
Expected: compile error, `writeup` items not found.

- [ ] **Step 3: Implement `writeup.rs`**

- **`parse`:** walk the lines, tracking fenced blocks (```` ``` ```` / `~~~`). Outside fences, a line starting with `##` or `###` is a heading. Normalise its text: lowercase it, replace `’` with `'`, trim it, and drop a trailing `:`. It starts the Idea section if it starts with `idea`, What worked if it starts with `what worked`, and What didn't if it starts with `what didn't` or `what did not`. Any other `#`/`##` heading ends the current section. Before the emptiness check, strip HTML comments; a section that's then whitespace-only is `""`.
- **`sanitize`:** outside fences and inline code spans, replace `<` with `&lt;` and `@` with `@\u{200B}`, and prefix `###` to any line starting with `#`, so every heading is at least `####`. Inside code, leave the text unchanged; GitHub renders it literally and mentions inside code don't notify. Finally cap the text at 8,000 chars plus `…(truncated)`.
- **`status_comment`:**
  - The first line is the marker: `<!-- cb:writeup thread=N -->` when a thread is known, `<!-- cb:writeup -->` otherwise.
  - Then `### Write-up`, followed by one of:
    - `Write-up complete.`
    - `**Not ready to merge:** write-up missing: <missing joined by ", ">.` when the claim is `valid_claim_better`
    - `Write-up missing: <…>. Every submission needs one.`
  - When the write-up is incomplete, add a line: add the three headings to the PR description, and this comment updates on edit.
  - Then `Discussion: https://github.com/<repo>/discussions/N` when a thread is known.

- [ ] **Step 4: Run the tests and check they pass**

Run: `cargo test -p cb writeup`
Expected: all 8 pass.

- [ ] **Step 5: Commit**

```bash
git add crates/cb/src/writeup.rs crates/cb/src/main.rs
git commit -m "cb: parse and render submission write-ups"
```

### Task 2: Thread rendering and the `cb discussion` / `cb writeup` commands

**Files:**
- Create: `crates/cb/src/discussion.rs` (tests in-module)
- Modify: `crates/cb/src/pipeline.rs:688` (`fn md` → `pub(crate) fn md`)
- Modify: `crates/cb/src/main.rs` (`mod discussion;`, two subcommands, dispatch)

**Interfaces:**
- Consumes: `writeup::{parse, render, status_comment, Writeup}`, `pipeline::{md, read_claim}`.
- Produces (Rust):
  - `pub fn title(gate: &Value, pr: u64, pr_title: &str) -> String`: `[<slug>] PR #<n>: <md(title)>`, or `[refused] …` when `gate["ok"] != true`; capped at 256 chars.
  - `pub fn facts(gate: &Value, claim: Option<&Value>, entry: Option<&Value>, pr: u64, commit: &str) -> String`: wrapped in `<!-- cb:facts -->` … `<!-- /cb:facts -->`. It gives the PR as `#<n>`, the commit's first 7 chars, and the same status, value and reason fields and the same `md()` cleanup as `pipeline::comment`; a refused gate lists its errors.
  - `pub fn opening(facts: &str, w: &Writeup) -> String` = `facts + "\n" + render(w)`
  - `pub fn refresh(current: &str, w: &Writeup) -> String`: keeps `current` up to its **first** `<!-- cb:writeup -->` and replaces the rest with `render(w)`. If the marker is missing, it appends `render(w)`.
- Produces (CLI, used by Tasks 3–4):
  - `cb discussion open --gate G [--score S] [--entry E] --pr N --title-file T --commit SHA --body-file B --out DIR` writes `DIR/title.txt`, `DIR/opening.md` and `DIR/reply.md` (reply = the facts block alone).
  - `cb discussion refresh --current C --body-file B` prints the new opening post.
  - `cb writeup --body-file B [--claim STATUS] --repo OWNER/NAME [--thread N]` prints the status comment.
  - `--entry` takes the same `cb record` output file as `cb comment`, and uses its `["entry"]`. A missing or `null` body file counts as an empty description.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn refused_gate_title_and_facts() {
    let g = json!({"ok": false, "errors": ["symlink: x"]});
    assert_eq!(title(&g, 9, "try @x"), "[refused] PR #9: try  x");
    assert!(facts(&g, None, None, 9, "abcdef0123").contains("symlink: x"));
}
#[test] fn title_uses_slug() {
    let g = json!({"ok": true, "slug": "ledger-cbor"});
    assert!(title(&g, 78, "inline").starts_with("[ledger-cbor] PR #78: inline"));
}
#[test] fn facts_neutralise_claim_text() {
    let g = json!({"ok": true, "slug": "s"});
    let c = json!({"status": "valid", "value": 1.5, "expr": "speedup", "reason": "<img> @everyone"});
    let f = facts(&g, Some(&c), None, 1, "abcdef0123");
    assert!(f.starts_with("<!-- cb:facts -->") && f.contains("abcdef0") && !f.contains("@everyone") && !f.contains("<img"));
}
#[test] fn refresh_replaces_only_the_writeup() {
    let w1 = parse("## Idea\nold\n");
    let w2 = parse("## Idea\nnew\n## What worked\nb\n## What didn't\nc\n");
    let o = opening("<!-- cb:facts -->F<!-- /cb:facts -->", &w1);
    let r = refresh(&o, &w2);
    assert!(r.starts_with("<!-- cb:facts -->F") && r.contains("new") && !r.contains("old"));
}
#[test] fn user_text_cannot_forge_a_marker() {
    let hostile = parse("## Idea\n<!-- cb:writeup -->\nFAKE\n## What worked\nb\n## What didn't\nc\n");
    let o = opening("<!-- cb:facts -->F<!-- /cb:facts -->", &hostile);
    assert_eq!(o.matches("<!-- cb:writeup -->").count(), 1);
}
```

- [ ] **Step 2: Run the tests and check they fail**

Run: `cargo test -p cb discussion`
Expected: compile error.

- [ ] **Step 3: Implement `discussion.rs` and wire up the two subcommands in `main.rs`**

Follow the existing `Command::Comment` pattern: clap `Subcommand` variants, errors as `Err(String)`. `Discussion` has nested `Open` / `Refresh` subcommands.

- [ ] **Step 4: Run the tests and a CLI smoke test**

Run: `cargo test -p cb`, then

```bash
printf '{"ok":true,"slug":"s"}' > /tmp/g.json; printf '## Idea\na\n' > /tmp/b.md; printf 'T' > /tmp/t.txt
./target/debug/cb discussion open --gate /tmp/g.json --pr 1 --title-file /tmp/t.txt --commit abcdef0 --body-file /tmp/b.md --out /tmp/d && cat /tmp/d/title.txt
./target/debug/cb writeup --body-file /tmp/b.md --claim valid_claim_better --repo o/r --thread 3 | head -3
```

Expected: every test passes (including the existing ones), `[s] PR #1: T`, and a status comment that starts `<!-- cb:writeup thread=3 -->` and contains `Not ready to merge`.

- [ ] **Step 5: Commit**

```bash
git add crates/cb/src/discussion.rs crates/cb/src/pipeline.rs crates/cb/src/main.rs
git commit -m "cb: render discussion threads and write-up status (cb discussion, cb writeup)"
```

### Task 3: GitHub helper and the Comment workflow step

**Files:**
- Create: `.github/scripts/discussion.sh`
- Modify: `.github/workflows/comment.yml` (permissions; one new step after "Record the PR attempt and render the comment")

**Interfaces:**
- Consumes: the CLI from Task 2.
- Produces: `discussion.sh <subcommand>`. It reads `GH_TOKEN` and `REPO` (owner/name) from the environment, and every subcommand exits non-zero on API errors.
  - `status-comment PR` prints `<comment-id> <thread-number-or-empty>` for the **oldest** comment by `github-actions[bot]` whose body starts with `<!-- cb:writeup`, or nothing.
  - `upsert-status PR FILE` edits that comment (`PATCH /repos/$REPO/issues/comments/<id>`) or creates it (`gh pr comment`).
  - `create TITLE_FILE BODY_FILE` looks up the repository id and the `Submissions` category id by name, calls `createDiscussion`, and prints the new number. If the category is missing, it exits 1 with `no "Submissions" discussion category`.
  - `reply NUM FILE` → `addDiscussionComment`.
  - `get-body NUM` prints the discussion body; `set-body NUM FILE` → `updateDiscussion`.

- [ ] **Step 1: Write `discussion.sh`**

Use `set -euo pipefail`. Pass all values to `gh api graphql` with `-F`/`-f` (bodies with `-F body=@FILE`); nothing is interpolated into the query text. Filter comment authors on `.user.login == "github-actions[bot]"`, so a PR author can't plant a fake status comment.

- [ ] **Step 2: Lint it**

Run: `shellcheck .github/scripts/discussion.sh`
Expected: no output.

- [ ] **Step 3: Add the step to `comment.yml`**

- Add `discussions: write` to the workflow `permissions`. `pull-requests: write` already covers editing PR comments.
- The new step runs `if: always()` with `continue-on-error: true`, so a failure here never fails the job and never touches the score comment or results commit. It sets env `REPO: ${{ github.repository }}`, plus the `PRS`, `HEAD_SHA` and `GH_TOKEN` the previous step uses. Logic:
  1. Work out `PR` the same way the previous step does. Shell variables don't carry across steps, so re-derive the gate file: `artifacts/gate/gate.json`, else `gate-missing.json`. Use `artifacts/score/score.json` and `entry.json` only if they exist.
  2. `gh pr view "$PR" -R "$REPO" --json body,title --jq .body > pr-body.md`, and the title into `pr-title.txt`. Verified: `gh --jq` prints a `null` body as an empty line.
  3. Claim status: `jq -r '.entry.status // empty' entry.json`, if `entry.json` exists.
  4. `cb discussion open` with the gate, score and entry files the previous step used (skip `--score`/`--entry` when they're absent), with output in `disc/`.
  5. `read -r cid thread < <(discussion.sh status-comment "$PR")`. If there's no thread, `thread=$(discussion.sh create disc/title.txt disc/opening.md)`. Otherwise run `discussion.sh reply "$thread" disc/reply.md`, then `get-body` → `cb discussion refresh` → `set-body`.
  6. `cb writeup --body-file pr-body.md --claim "$claim" --repo "$REPO" --thread "$thread"` → `discussion.sh upsert-status "$PR"`.
  7. On any failure, print `::warning::discussion step failed: <reason>`.

- [ ] **Step 4: Lint the workflow**

Run: `ruby -ryaml -e 'YAML.load_file(ARGV[0])' .github/workflows/comment.yml`. `actionlint` isn't installed here; if the maintainer agrees to install it (`brew install actionlint`), run it too, since it catches expression and permission mistakes that a YAML load doesn't.
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add .github/scripts/discussion.sh .github/workflows/comment.yml
git commit -m "Comment: open or update a discussion thread per submission and post the write-up status"
```

### Task 4: `writeup.yml`: react to PR description edits

**Files:**
- Create: `.github/workflows/writeup.yml`

**Interfaces:**
- Consumes: `cb discussion refresh`, `cb writeup`, `discussion.sh status-comment|get-body|set-body|upsert-status`.

- [ ] **Step 1: Write the workflow**

- **Trigger and permissions:**
  - `on: pull_request_target: types: [edited], branches: [main]`
  - `permissions: contents: read, pull-requests: write, discussions: write`
  - `concurrency: group: writeup-${{ github.event.pull_request.number }}, cancel-in-progress: true`
- **Job conditions:** `github.event.changes.body != null && github.event.pull_request.base.ref == 'main'`, with `timeout-minutes: 10`.
- **Steps:**
  1. Check out `main`, with `persist-credentials: false`.
  2. `rustup toolchain install`, then `cargo build --locked --release`. No cache: see Global Constraints.
  3. Write the body from `github.event.pull_request.body`, passed through `env` (never interpolated into the script), to `pr-body.md`.
  4. Claim status: `jq -r --arg c "$HEAD_SHA" 'select(.stage=="pr" and .commit==$c) | .status' challenges/*/results.jsonl | tail -1`.
  5. Read `cid thread` with `status-comment`. If there's **no** status comment, stop: Comment hasn't run yet, and it reads the current description when it does. Only Comment ever creates the status comment, so two workflows can't race to create two (which could later lead to two threads).
  6. If there's a thread: `get-body` → `cb discussion refresh` → `set-body`.
  7. `cb writeup …`, then edit the existing status comment (`upsert-status` finds it).

  Known and accepted: if an edit lands while a Comment run is between reading the description and posting, the last writer wins, and the status can lag until the next edit or push.

- [ ] **Step 2: Lint the workflow**

Run: the same lint as in Task 3, on `.github/workflows/writeup.yml`.
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/writeup.yml
git commit -m "writeup: refresh the thread and write-up status when a PR description is edited"
```

### Task 5: Templates and participant docs

**Files:**
- Create: `.github/pull_request_template.md`
- Modify: `crates/cb/src/challenge.rs:340-352` (`fn submitting`), plus its existing scaffold test if one asserts on the text
- Modify: `challenges/{golomb-14,lean-sum,ledger-cbor,pedersen,sortnet-8}/SUBMITTING.md`
- Modify: `challenges/ledger-cbor/PROMPT.md` (step 4 of "How to work")
- Modify: `docs/pipeline.md` (the Comment line in the diagram, plus a short "Discussions" section)

- [ ] **Step 1: PR template**

Write the three required headings, each followed by an HTML-comment placeholder. That way an untouched template still counts as missing, as Task 1's test expects. The placeholders:

- Idea: "What you changed and why you expected it to help."
- What worked: "What improved, with numbers if you measured any."
- What didn't: "What you tried that didn't help or was rejected, and why. Say so even if it's 'nothing else tried'."

- [ ] **Step 2: Docs text**

Add one paragraph, the same in the scaffold `submitting()` and each `SUBMITTING.md`: the PR description must contain `## Idea`, `## What worked` and `## What didn't`. `gh pr create --body` skips the template, so write them yourself. The bot copies them into the PR's thread under Discussions → Submissions, where others discuss it. A PR that claims a record isn't merged until all three are filled in.

In ledger-cbor's `PROMPT.md` step 4, replace the "In the description say …" sentence with: the description must use the three headings. Under What didn't, list approaches tried and abandoned. Before picking an idea, read the existing threads in the repo's Submissions discussions.

In `docs/pipeline.md`, add: the Comment step also opens or updates the discussion, and the write-up status is updated on edit by `writeup.yml`. Add the manual setup: enable Discussions and create the `Submissions` category.

- [ ] **Step 3: Run the tests**

Run: `cargo test -p cb`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add .github/pull_request_template.md crates/cb/src/challenge.rs challenges/*/SUBMITTING.md challenges/ledger-cbor/PROMPT.md docs/pipeline.md
git commit -m "Docs: require the Idea / What worked / What didn't write-up on every submission"
```

### Task 6: Setup and one live check (maintainer, billed)

The maintainer does the setup before the PR merges. The live check runs after the merge, because both workflows run main's copy. It costs one full Submission and Comment cycle on the cheapest challenge, plus one `writeup.yml` run, so say so before starting it.

- [ ] **Step 1: The maintainer enables Discussions and creates the `Submissions` category**

In Settings → General → Features → Discussions, enable Discussions. Then create a category named `Submissions`, of type Announcement.

- [ ] **Step 2: After the merge, open a test PR on the cheapest challenge with a hostile description**

The description has only `## Idea` (missing the other two), with `@Ryun1` and `<img src=x>` in it.
Expected:
- A thread `[<slug>] PR #N: …` appears in Submissions, with no live mention and no image.
- The status comment says `Write-up missing: What worked, What didn't`, or `Not ready to merge` if the PR claims a record.
- If the thread wasn't created and the log shows a permission error for the Announcement category, change the category to Open and re-run Comment. Record the outcome in `docs/pipeline.md`.

- [ ] **Step 3: Edit the PR description to add the other two sections**

Expected: `writeup.yml` runs in about a minute. Building `cb` took 40 s in the latest Comment run. The thread's write-up section updates, and the status comment says `Write-up complete.` Then close the test PR.
