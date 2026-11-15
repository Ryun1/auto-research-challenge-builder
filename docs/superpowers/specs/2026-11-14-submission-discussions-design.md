# Submission discussions: design

## Intent

Participants (people and agents) should be able to see what each attempt tried,
what worked and what didn't, so the next attempt builds on it instead of
repeating it. Failed attempts matter as much as records: ECDSA.fail's lesson
was that nobody counted the attempts that didn't work.

## Goals

- One GitHub Discussion thread per submission PR, opened automatically, for
  every outcome: record, slower, rejected by the gate.
- A required write-up in the PR description, with three fixed headings:
  `## Idea`, `## What worked`, `## What didn't`. The bot copies it into the
  thread, and participants discuss underneath.
- A record-claiming PR without a complete write-up is flagged **not ready to
  merge** in the score comment. The maintainer holds the merge until it's filled in.

## Non-goals

- Linking threads from the public site. The threads live on the private repo,
  so outsiders would get a 404.
- Per-challenge categories. GitHub's API can't create categories.
- Blocking non-record PRs. They get flagged in the thread, but nothing blocks on them.

## Anti-goals

- Extra billed Actions runs per submission. Minutes have already run out once.
- Running or building submission code anywhere new.
- Letting PR text notify people (`@` mentions), inject HTML, or pass itself off
  as the bot's score.

## Approaches

| Approach | Trade-off | Verdict |
|---|---|---|
| Extend `comment.yml` + a `cb discussion` command | No extra run per submission; follows the existing pattern (`cb` renders, the workflow posts) | **Chosen** |
| A separate `discussion.yml` triggered when Comment finishes | Keeps Comment simpler, but adds a billed run per submission | Rejected: minutes |
| A ready-made Marketplace action | Less of our own code, but third-party code with write access, and we'd still need find-or-update logic | Rejected |

## Design

**Manual setup (maintainer):** enable Discussions on
`Ryun1/auto-research-challenge-builder`, and create a category "Submissions"
of type Announcement (only maintainers and the bot start threads; anyone replies).

**Write-up format.** `cb` parses the PR description for the three headings.
"Complete" means each section is present and not empty after trimming, and
isn't the template's placeholder text.

- `.github/pull_request_template.md` offers the headings for PRs opened on the web.
- Each `challenges/*/SUBMITTING.md` and `PROMPT.md` (and the kit template they
  come from) states the headings as required. `gh pr create --body` bypasses
  the template, so these docs carry the rule for agents.

**`cb discussion`** (new subcommand, alongside `cb comment`). Inputs: the gate
and score artifacts, the attempt entry (as for `cb comment`), the PR number,
title and author, and the PR description from a file. It renders:

- a bot facts block: challenge, PR link, commit, outcome, score vs record;
- the write-up sections, or "write-up missing: <which sections>";
- a reply body for each later push (new commit, new score).

It also reports, for the workflow, whether the write-up is complete. Handling
of PR text: keep markdown, defuse `@` mentions (zero-width joiner), strip raw
HTML and HTML comments, and keep it under a fixed heading that follows the bot
block. This is gentler than `cb comment`'s neutraliser, which would mangle
identifiers like `decode_multiasset`.

**`cb comment` change.** It takes the write-up status and the thread URL.
It links the thread, and adds **not ready to merge: write-up missing** when the
PR claims a record and the write-up is incomplete.

**`comment.yml` change.** Add `discussions: write`. Reading the PR description
needs only read access, which `pull-requests: write` includes. After recording
the attempt:

1. `gh pr view $PR --json body,title,author` → files (data only).
2. Find the thread: look for the discussion URL in the bot's earlier PR
   comments (deterministic, no search-index lag; Comment is already serialised
   per PR).
3. None: `createDiscussion` in "Submissions", titled
   `[<slug>] PR #<n>: <PR title>`. Found: `addDiscussionComment` with the push
   update, and `updateDiscussion` to refresh the write-up in the opening post.
4. Post the PR score comment, now with the thread link.

A failure in the discussion step must not lose the score comment or the
results commit. It logs a warning, and the PR comment goes out without the link.

**New `writeup.yml`** on `pull_request_target: [edited]`, into `main`. Runs
from main, never checks out PR code, and needs no Rust build if parsing is
cheap in `cb`. If building `cb` is too slow there, reuse a cached build or do
the check with a small script. Builder's call; target about a minute. It
re-checks the headings, refreshes the thread's opening post, and edits the
bot's PR comment to add or clear the not-ready flag. It runs only when the body
changed, using `github.event.changes.body`. It has its own concurrency group,
so it never cancels Submission.

## Left to the builder

- Can `GITHUB_TOKEN` (github-actions) create discussions in an Announcement
  category? If not, use an Open category and say so in the setup note.
- The exact placeholder text and how "empty" is detected.
- Whether `writeup.yml` edits the existing score comment or posts a small new one.
- Tests: `cb discussion` rendering and neutralising (mentions, HTML, a forged
  facts block), heading parsing, and the not-ready flag in `cb comment`. Extend
  `scripts/malicious-prs.sh` with a hostile PR description.
