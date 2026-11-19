# Research Challenges Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `research` challenge type to `cb` end to end (spec, scaffold, gate, sandboxed measurement, recording, auto-landing workflows, preflight, feed, site) and ship the `coin-select` pilot.

**Architecture:** Everything research-specific lives in a new module tree `crates/cb/src/research/` (one file per stage). Existing simple-challenge code stays as it is; the entry points (`challenge`, `pipeline::gate`, `pipeline::comment`, `preflight::run_all`, `feed::feed`, `pages::challenge`, `main.rs`) dispatch on `[challenge] type`. The judge is trusted challenge code that drives the untrusted participant through `cb participant`.

**Tech Stack:** Rust 2021 (`serde`, `serde_json`, `toml`, `clap`, `libc` — no new crates), bash, GitHub Actions, Python 3 stdlib (pilot judge and baseline), vanilla JS + inline SVG (site).

**Spec:** `docs/superpowers/specs/2026-11-19-research-challenges-design.md` — read it first; section numbers below (§n) refer to it.

## Global Constraints

- No new Rust dependencies. `cargo fmt --all --check` and `cargo clippy --locked --all-targets -- -D warnings` stay clean; `cargo test --locked` passes on macOS (no sandbox) and on Linux CI (`CB_REQUIRE_SANDBOX=1`).
- Simple challenges behave exactly as before; every existing test keeps passing unchanged (only additive edits to existing tests, e.g. a new `type` key in gate JSON).
- Results rows and feeds for research carry `schema: 2` / `feed_schema_version: 2` and `type: "research"`.
- Untrusted text (submission.toml strings, judge details, reasons) is neutralised with `pipeline::md` in comments and rendered with `textContent` on the site.
- Submission ids, directions, taxonomy ids/values are slugs (`challenge::is_slug`).
- Code comments follow the repo's style: short, say why, cite the spec section or issue (#90) where it helps.
- Commit after each task with a message `research: <what>` (no `[skip ...]` markers anywhere in messages).

## Review Focus

1. A submission PR that also touches a file outside its dir, or edits/deletes an existing submission, must be refused by the gate — test in Task 3.
2. The build must not be able to modify the judge (`contracts/`, `harness/`) in the workspace before the judge runs — sandbox probe in Task 8, and Task 5 builds with `dir = ws` read-only except `submissions/<id>`.
3. A participant that hangs, never replies, or exceeds memory/CPU must end as `rejected` with a reason, never hang the job or become `infra_failed` — Task 4 (timeout/cpu tests) and Task 9 (judge reply timeout).
4. A judge whose output omits a mechanical property or an axis must give `infra_failed`, not `valid` — Task 5.
5. Two main runs whose axes disagree beyond tolerance must not be `accepted` — Task 6.

---

### Task 1: Research spec, taxonomy and declaration types

**Files:**
- Create: `crates/cb/src/research/mod.rs` (declares submodules), `crates/cb/src/research/spec.rs`
- Modify: `crates/cb/src/challenge.rs` (Meta gets `type`; `Kind`, `load_any`, `challenge_type`), `crates/cb/src/main.rs` (`mod research;`)

**Interfaces:**
- Produces (in `research::spec`):
  - `pub struct ResearchSpec { pub challenge: Meta, pub commands: Commands, pub submissions: Submissions, #[serde(default)] pub limits: Limits, pub contracts: Vec<Contract>, #[serde(default)] pub properties: Vec<Property>, pub axes: Vec<Axis>, pub taxonomy: TaxonomyRef, #[serde(skip)] pub tax: Taxonomy }` — fields exactly as spec §1 (`Meta {slug, kind (serde rename "type"), status, runner}`, `Commands {setup}`, `Submissions {max_bytes, #[default] toolchains, #[default] writable}`, `Limits {cpu_secs, memory_mib, processes, file_mib, wall_secs}` with `Default` 60/1024/32/64/120, `Contract {id, status, judge: Option, interface: Option, choice_space: Option, admits_when: Option}`, `Property {id, check, contract}`, `Axis {id, unit, direction, repeats (default 1), tolerance (default 0.0), contracts (default empty)}`, `TaxonomyRef {file}`); all `#[serde(deny_unknown_fields)]`, all `Clone + Debug + Serialize + Deserialize`.
  - `impl ResearchSpec { pub fn parse(text: &str) -> Result<Self, String>; pub fn load(dir: &Path) -> Result<Self, String> /* also loads tax from taxonomy.file, checks slug == dir name */; pub fn contract(&self, id: &str) -> Option<&Contract>; pub fn axes_for(&self, contract: &str) -> Vec<&Axis> /* axes with empty contracts or containing it */; pub fn properties_for(&self, contract: &str) -> Vec<&Property>; pub fn limits_json(&self) -> serde_json::Value }`
  - `pub struct Taxonomy { pub version: u32, pub axes: Vec<TaxAxis>, #[default] pub rules: Vec<Rule> }`, `TaxAxis {id, definition, values: Vec<TaxValue>}`, `TaxValue {id, definition}`, `Rule {text}`; `Taxonomy::parse(text) -> Result<Self,String>` (unique slug ids); `Default`.
  - `pub struct Declaration { pub id, pub contract, pub novelty, #[default] pub direction, #[default] pub parent, #[default] pub taxonomy: BTreeMap<String,String>, pub summary, #[default] pub claims: Vec<Claim>, #[default] pub model }`, `Claim {property, argument}`; `Declaration::parse(text) -> Result<Self,String>`; `pub fn check(&self, spec: &ResearchSpec, dir_name: &str) -> Vec<String>` (static rules of spec §2/§3: id == dir name and slug, contract exists and `open`, novelty `new` ⇒ direction slug + empty parent, `builds-on` ⇒ parent slug + empty direction, taxonomy keys known and values slugs, claims only for `declared` properties of its contract, caps 500/2000/100/16/16); `pub fn proposed(&self, tax: &Taxonomy) -> Vec<String>` (keys whose value is not listed); `pub fn to_json(&self) -> Value`.
- Produces (in `challenge`): `pub enum Kind { Simple(Spec), Research(ResearchSpec) }`, `pub fn load_any(dir: &Path) -> Result<Kind, String>`, `pub fn challenge_type(dir: &Path) -> Result<String, String>` (reads only `[challenge].type`, default `"simple"`). `Meta` gains `#[serde(rename = "type", default)] pub kind: Option<String>`; `Spec::validate` refuses `Some(x)` unless `x == "simple"`, and `Spec::load` on a research dir errors with `"...: a research challenge (type = \"research\"): use load_any"`.

- [ ] **Step 1: Tests** in `research/spec.rs` `mod tests`: `valid_research_spec_parses` (a const `GOOD` TOML like spec §1 with one open, one deferred contract, one property, two axes); `deferred_contract_needs_admits_when`; `open_contract_needs_judge`; `bad_values_refused` (one `bad(from,to)` helper like `challenge::tests::validation`: `-latest` runner, `repeats = 0`, `tolerance = 1.5`, direction `"up"`, duplicate axis id, property naming an unknown contract, contract id `"wallet"` without `@N`, an unknown key); `declaration_rules` (new without direction; builds-on with direction; id ≠ dir; claim on a mechanical property; unknown taxonomy key; deferred contract — each yields a non-empty `check()`); `proposed_values` (known key with unlisted value is proposed); in `challenge::tests`: `simple_spec_unchanged` (`spec_toml("x","perf",None)` parses, with and without `type = "simple"`), `research_type_is_not_a_simple_spec`.
- [ ] **Step 2:** `cargo test --locked research::spec` — fails (module missing).
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --locked` — all pass; clippy clean.
- [ ] **Step 5:** Commit `research: spec, taxonomy and submission declaration types`.

### Task 2: `cb new --type research` scaffold

**Files:**
- Create: `crates/cb/src/research/scaffold.rs`
- Modify: `crates/cb/src/main.rs` (`New { family: Option<Family>, #[arg(long = "type", value_enum, default_value = "simple")] kind: ChallengeType }`; family required when simple), `crates/cb/src/challenge.rs` (export `THREAT_MODEL` untouched)

**Interfaces:**
- Consumes: Task 1 `ResearchSpec::load`.
- Produces: `pub fn scaffold(root: &Path, slug: &str) -> Result<PathBuf, String>` writing the file set of spec §9 (open contract `main@1` with judge `contracts/main/judge`; deferred example `streaming@0`; property `correct` mechanical; axes `time_ms` minimise repeats 3 tolerance 0.2, `size_bytes` minimise repeats 1 tolerance 0.0). `pub const THREAT_MODEL_RESEARCH: &str` (spec §5 questions as `- ...:` bullets under `## ` sections, same format the existing gap checker reads). The judge, `submissions/baseline/build.sh` and `setup.sh` are executable; the judge fails closed (prints `{"ok": false, "inputs": "", "checks": [], "axes": {}}` with exit 1). `SUBMITTING.md` and `PROMPT.md` explain: one new dir `submissions/<id>/`, `submission.toml` fields, novelty, building on a parent by copying its dir, what is measured, no record, valid PRs land automatically.

- [ ] **Step 1: Test** `scaffold_research_loads`: scaffold into a temp root, `ResearchSpec::load` succeeds, the listed files exist, the judge is executable and exits non-zero, `Declaration::parse` of `submissions/baseline/submission.toml` passes `check(&spec, "baseline")`, scaffolding twice errors, `challenge_type` says `"research"`.
- [ ] **Step 2:** run, fails. **Step 3:** implement + CLI. **Step 4:** `cargo test --locked`; `cargo run -q -- new toy --type research --root $(mktemp -d)` prints `created ...`; `cargo run -q -- new toy2 --root /tmp/x` (no family, simple) errors with `--family is required for simple challenges`.
- [ ] **Step 5:** Commit `research: cb new --type research`.

### Task 3: Research gate

**Files:**
- Create: `crates/cb/src/research/gate.rs`, `crates/cb/src/research/results.rs`
- Modify: `crates/cb/src/pipeline.rs` (`Gate` gains `kind: Option<String>`, `submission: Option<String>`, `declaration: Option<Value>`; `json()` emits `"type"`, `"submission"`, `"declaration"`; after the slug is known, `challenge_type` decides; research → `research::gate::check`)

**Interfaces:**
- Produces (`research::results`): `pub fn accepted(results: &[Value]) -> BTreeMap<String, Value>` (per submission id, its latest row with stage `accept|baseline`, kept only if that row's status is `accepted`); `pub fn direction_of(results: &[Value], id: &str) -> Option<String>`.
- Produces (`research::gate`): `pub fn check(g: &mut pipeline::Gate, repo: &Path, root: &str, slug: &str, spec: &ResearchSpec, merge_base: &str, base: &str, head: &str, out: &Path)` — applies spec §3 in order, pushing every refusal to `g.errors`, sets `g.submission`, `g.declaration`, exports on success.
- Error strings tests rely on (substrings): `"may only add files under"`, `"already exists"`, `"symlink"`, `"gitlink"`, `"bytes > max_bytes"`, `"submission.toml"`, `"build.sh"`, `"not accepted on main"`, `"direction"`, `"deferred"`, `"not open"`.

- [ ] **Step 1: Tests** in `research/gate.rs` using a scratch git repo (pattern of `pipeline::tests::sh`): a fixture fn makes a repo with `challenges/toy` scaffolded by Task 2 (status set to `open`), a `results.jsonl` with an accepted `baseline` row, commits it as `main`. Cases, each on its own branch, asserting `g.errors` (empty or containing the substring): `adds_one_submission_ok` (exports `out/challenges/toy/submissions/s1/submission.toml`, gate JSON has `type: "research"`, `submission: "s1"`); `touches_outside_dir`; `edits_existing_submission` (modify `submissions/baseline/build.sh`); `deletes_a_file`; `two_submission_dirs`; `symlink_in_dir`; `gitlink_in_dir`; `size_cap`; `missing_build_sh`; `build_sh_not_executable`; `forged_id` (toml id ≠ dir); `unknown_parent`; `parent_not_accepted` (dir exists, no accepted row); `builds_on_accepted_parent_ok`; `direction_taken`; `deferred_contract`; `challenge_not_open`; `writes_measure_json` (a file `challenges/toy/measure.json` → outside); `edits_judge` (`contracts/main/judge`). Plus in `pipeline::tests`: existing simple gate tests still pass and their JSON has `type: "simple"`.
- [ ] **Step 2:** run, fail. **Step 3:** implement. **Step 4:** `cargo test --locked`. **Step 5:** Commit `research: gate`.

### Task 4: `cb participant` launcher

**Files:**
- Create: `crates/cb/src/research/participant.rs`
- Modify: `crates/cb/src/main.rs` (`Participant { #[arg(long)] report: Option<PathBuf>, #[arg(last = true, required = true)] cmd: Vec<String> }`, exits with the participant's code), `crates/cb/src/preflight.rs` (make `sq` and `build_sandbox_available` `pub(crate)` if not already)

**Interfaces:**
- Produces: `pub struct Limits { cpu_secs, memory_mib, processes, file_mib, wall_secs: u64 }` read via `pub fn limits_from_env() -> Limits` (`CB_LIMITS` JSON, else `research::spec::Limits::default()` values); `pub struct Report { pub status: String, pub exit_code: Option<i32>, pub signal: Option<i32>, pub wall_secs: f64, pub cpu_secs: f64, pub peak_rss_mib: f64 }` (`Serialize`); `pub fn run(cmd: &[String], limits: &Limits, sandbox: bool) -> Result<Report, String>` (stdio inherited); `pub fn sandbox_script(cwd: &Path, limits: &Limits, cmd: &[String]) -> String` (spec §4 sandbox steps, pure, testable); `pub fn exit_code(r: &Report) -> i32` (exit code, else 128+signal, 124 for timeout).
- Plain mode: `pre_exec` sets `setpgid(0,0)`, `RLIMIT_CPU`, `RLIMIT_FSIZE`, and on Linux `RLIMIT_AS`; the parent polls `wait4(pid, WNOHANG, &rusage)` and at `wall_secs` kills `-pgid` with SIGKILL → `status "timeout"`. `cpu_limit` when killed by SIGXCPU/SIGKILL with `cpu_secs >= limit - 0.5`. Peak RSS from `ru_maxrss` (bytes on macOS, KiB on Linux).
- Sandbox mode: `sudo -n unshare --mount --net --pid --fork --kill-child --mount-proc -- bash -c <sandbox_script>`; same wait4/report; `timeout` exit 124 → `timeout`.

- [ ] **Step 1: Tests**: `sandbox_script_shape` (contains `mount --bind`, `/mnt`, `ro`, `tmpfs`, `/tmp`, `prlimit --cpu=`, `--as=`, `--nproc=`, `--fsize=`, `setpriv --reuid=65534`, `env -i`, `timeout`; the cmd is single-quoted, so `a'b` survives); `plain_ok_exit_code` (`sh -c 'exit 3'` → status `failed`, exit_code 3); `plain_timeout` (`sleep 30` with `wall_secs = 1` → `timeout`, wall < 5 s); `plain_cpu_limit` (`sh -c 'while :; do :; done'`, `cpu_secs = 1` → `cpu_limit`); `plain_peak_rss` (`python3 -c "b=bytearray(200*1024*1024)"` → `peak_rss_mib >= 150`); `sandboxed_runs_as_nobody_without_network` — only when `build_sandbox_available()` (assert it is when `CB_REQUIRE_SANDBOX=1`): `id -u` prints 65534, writing `./x` fails, `/tmp` is empty, `(exec 3<>/dev/tcp/1.1.1.1/443)` fails.
- [ ] **Step 2–4:** fail, implement, pass (`cargo test --locked research::participant`). **Step 5:** Commit `research: cb participant launcher`.

### Task 5: `cb measure`

**Files:**
- Create: `crates/cb/src/research/measure.rs`, `crates/cb/tests/research_measure.rs` (integration test: uses `env!("CARGO_BIN_EXE_cb")`)
- Modify: `crates/cb/src/main.rs` (`Measure { slug, #[arg(long, default_value="challenges")] root, #[arg(long, conflicts_with="id")] submission: Option<PathBuf>, #[arg(long)] id: Option<String>, #[arg(long)] sandbox: bool, #[arg(long, default_value="measure.json")] out: PathBuf }`)

**Interfaces:**
- Consumes: Task 1 types, Task 4 (`CB_BIN` points at it), `preflight::{run_in, run_mode, Mode, build_sandbox_available, seed, host, host_policy}`, `paths::scratch`.
- Produces: `pub enum Source<'a> { Export(&'a Path) /* gate --out dir */, Id(&'a str) /* already in the challenge */, Dir(&'a Path) /* any submission dir; id = its name */ }`; `pub fn measure(root: &Path, slug: &str, source: Source, sandbox: bool, seed: Option<u64>) -> Result<Value, String>` producing spec §4 step 8 JSON; `pub fn judge_env(...)` internal. `CB_BIN` is `std::env::var("CB_BIN")` if set (tests), else `std::env::current_exe()`.
- Status logic exactly spec §4 steps 3–7 (sandbox unavailable under `--sandbox` → `infra_failed`; build fails → `rejected` `build: ...`).

- [ ] **Step 1: Tests** (unit, in `measure.rs`, toy challenge = Task 2 scaffold with the judge replaced by a bash script that reads `$CB_SUBMISSION_DIR/answer` and prints judge JSON; no participant launcher needed): `valid_submission_measures` (status `valid`, axes `time_ms.n == 3`, `size_bytes.n == 1`, seed is a string, `build.ok`); `build_failure_rejected`; `failing_check_rejected` (reason contains the property); `missing_mechanical_property_is_infra`; `missing_axis_is_infra`; `judge_not_json_is_infra`; `inputs_differ_across_repeats_is_infra`; `verdicts_differ_across_repeats_rejected`; `seed_override_used`; `export_source_reads_gate_layout`. Integration test `measure_drives_participant`: a toy whose judge calls `"$CB_BIN" participant --report r.json -- ./answer.sh`, reads the report and emits `peak_rss_mib`; run `cb measure toy --root <tmp> --id baseline --out m.json` as a process; status `valid`.
- [ ] **Step 2–4:** fail, implement, pass. **Step 5:** Commit `research: cb measure`.

### Task 6: Record and comment for research

**Files:**
- Create: `crates/cb/src/research/record.rs`, `crates/cb/src/research/comment.rs`
- Modify: `crates/cb/src/main.rs` (`Record` gets `#[arg(long)] declaration: Option<PathBuf>`; stage validation by type: simple `baseline|promotion|pr`, research `pr|accept|baseline`; research prints `{"entry": row, "submission": id}`), `crates/cb/src/pipeline.rs` (`comment` dispatches to `research::comment::comment` when `gate["type"] == "research"`; make `md` `pub(crate)`; `read_results` reused)

**Interfaces:**
- Produces: `pub fn record(root: &Path, slug: &str, decl: &Declaration, runs: &[Value], a: &pipeline::Attempt) -> Result<Value, String>` (spec §6 statuses and row; appends to `results.jsonl`); `pub fn comment(gate: &Value, claim: Option<&Value>, entry: Option<&Value>) -> String` (starts with `<!-- cb:score -->`, spec §6 content and footer verbatim).

- [ ] **Step 1: Tests**: `pr_row_valid` (row fields per spec, `schema: 2`, `direction` resolved); `builds_on_inherits_direction`; `accept_two_valid_agreeing_runs_accepted` (axes combine samples: n = 2·repeats); `accept_axes_disagree_infra` (fees 10 vs 12, tolerance 0.05 → `infra_failed`, reason names `fees`); `accept_exact_axis_tolerance_zero`; `accept_rejected_wins_over_valid`; `accept_verdicts_differ_infra`; `baseline_twice_refused`; `accept_refuses_baseline_id`; `runs_for_other_submission_refused`; comment: `research_comment_renders_tables` (contains axis ids, `unverified`, the footer); `research_comment_neutralises_untrusted` (summary with `<script>` and `@user` has no `<` or `@`); `refused_gate_comment_unchanged`.
- [ ] **Step 2–4:** fail, implement, pass. **Step 5:** Commit `research: record and comment`.

### Task 7: Workflows and ops scripts

**Files:**
- Create: `.github/workflows/accept.yml`
- Modify: `.github/workflows/submission.yml`, `.github/workflows/comment.yml`, `.github/workflows/promote.yml`, `.github/workflows/ci.yml`, `.github/workflows/site.yml`, `scripts/audit-main.sh`

**Interfaces:** spec §6 "Workflows" is the contract. Reuse the existing patterns verbatim where they apply: pinned action SHAs, `rustup toolchain install`, `cargo build --locked --release`, the deploy-key checkout (`environment: results`, `ssh-key: ${{ secrets.RESULTS_DEPLOY_KEY }}`), the 5-try `pull --rebase`/push loop with jitter, `persist-credentials: false` on untrusted-adjacent checkouts, env vars for every `${{ }}` value used in `run:`.

- [ ] **Step 1:** submission.yml: gate outputs add `type`; the score step branches on it.
- [ ] **Step 2:** comment.yml: `fetch-depth: 0`; research branch records with `--declaration artifacts/gate/submission/challenges/$slug/submissions/$id/submission.toml`, commits results (`[skip actions]`), pushes; if `entry.status == "valid"` lands per spec §6 (re-gate into `land/`, verify `type`/`slug`/`submission` match, `gh pr view --json state,headRefOid` says OPEN at `$HEAD_SHA`, `git checkout "$HEAD_SHA" -- <dir>`, commit with trailers, separate push loop, `gh pr close --comment`). Simple path unchanged.
- [ ] **Step 3:** accept.yml per spec §6 (detect / run matrix [1,2] with one infra retry / record with per-slug concurrency, removal on rejected accept).
- [ ] **Step 4:** promote.yml detect skips research; ci.yml paths-ignore; site.yml `workflow_run` adds `Accept`; audit-main.sh allows `submissions/<id>/` changes and Accept runs (push on main, or owner dispatch).
- [ ] **Step 5:** Verify: `actionlint` if installed (else `python3 -c 'import yaml,sys;[yaml.safe_load(open(f)) for f in sys.argv[1:]]' .github/workflows/*.yml`), `bash -n scripts/audit-main.sh`. Commit `research: workflows (measure, auto-land, accept)`.

### Task 8: Research preflight

**Files:**
- Create: `crates/cb/src/research/preflight.rs`
- Modify: `crates/cb/src/preflight.rs` (`run_all` dispatches on `challenge_type`; expose `threat_model_gaps`, `du_bytes`, `max_child_rss_gb`, `Report::push` as `pub(crate)`)

**Interfaces:**
- Consumes: Task 5 `measure(.., Source::Id/Dir, sandbox, Some(seed))`.
- Produces: `pub fn run_all(dir: &Path) -> Result<preflight::Report, String>` with check names exactly: `threat model`, `contracts`, `taxonomy`, `build sandbox`, `baseline`, `determinism`, `salted inputs`, `axis noise: <axis>` (one per axis), `broken: <name>`, `hostable` (spec §7). The build-sandbox probe runs `Mode::Build` with `dir = ws`, `outputs = ["submissions/baseline"]` and asserts: `contracts/` not writable, home not writable, no sudo, no network, `submissions/baseline/x` writable, overlay write discarded.

- [ ] **Step 1: Tests**: a toy (Task 2 scaffold + working bash judge from Task 5's test helper — move that helper to `research/testkit.rs` `#[cfg(test)] pub fn toy(root, slug, judge: &str) -> PathBuf`) with broken variants that fail and a `self-report` variant that passes: `toy_preflight_passes` (not blocked); `broken_variant_accepted_blocks` (an `off-by-one` that passes → `broken: off-by-one` Fail); `self_report_far_from_baseline_blocks`; `noisy_axis_over_tolerance_blocks` (judge emits a random axis; detail suggests a tolerance); `deferred_listed` (contracts detail names `admits_when`); `scaffold_blocks_until_written` (fresh scaffold → blocked).
- [ ] **Step 2–4:** fail, implement, pass. **Step 5:** Commit `research: preflight`.

### Task 9: Pilot challenge `coin-select`

**Files:**
- Create: `challenges/coin-select/` — `challenge.toml`, `taxonomy.toml`, `setup.sh`, `README.md`, `SUBMITTING.md`, `PROMPT.md`, `THREAT_MODEL.md` (answered), `contracts/wallet/{judge,sim.py,INTERFACE.md,CHOICES.md}`, `submissions/baseline/{submission.toml,build.sh,select,README.md}`, `preflight/broken/{off-by-one,partial,constant,self-report}/...` (whole submissions), `results.jsonl` absent (status `draft`).

**Interfaces:** spec §10 is the contract (protocol, size model, properties, axes, scenarios). `judge` is `#!/usr/bin/env python3`, stdlib only, reads env per spec §4, runs `"$CB_BIN" participant --report <private tmp>/r.json -- ./select` with cwd `$CB_SUBMISSION_DIR` once per scenario, keeps its own temp files in `tempfile.mkdtemp()` (0700), never trusts anything the participant prints except protocol replies, enforces the 10 s reply timeout with `select`, and on any protocol violation stops that scenario, marks `valid-txs` failed with a short detail, and closes the participant. `inputs` = sha256 of the canonical JSON of all scenarios. Axes per spec §10; `fees` in ADA (float), `latency_ms` median over all pay replies, `peak_rss_mib` max over scenarios.

- [ ] **Step 1:** Write `sim.py` (scenario generator from seed, size/fee/min-ada model, validator, largest-first reference) with a self-test: `python3 contracts/wallet/sim.py --self-test` checks a hand-built valid and invalid tx and that 5 generated scenarios are fundable by largest-first.
- [ ] **Step 2:** judge + baseline + broken variants + docs + taxonomy (`strategy`: largest-first, random-improve, branch-and-bound, knapsack, exhaustive, other; `change`: single, split, payment-shaped, none-when-possible; `consolidation`: none, opportunistic, threshold; `language`: python, rust, c, go, other).
- [ ] **Step 3:** `cargo run -q -- preflight coin-select` → not blocked except `build sandbox` Warn on macOS; set each axis `tolerance` from the reported suggestion (round up generously for `latency_ms`/`peak_rss_mib`, since CI runners are noisier than a laptop).
- [ ] **Step 4:** `cargo run -q -- measure coin-select --id baseline --out /tmp/m.json` → `valid`, all five axes present.
- [ ] **Step 5:** Commit `research: coin-select pilot (draft)`.

### Task 10: Research feed

**Files:**
- Create: `crates/cb/src/research/feed.rs`
- Modify: `crates/cb/src/feed.rs` (`feed()` dispatches on type; `build()` handles both)

**Interfaces:** `pub fn feed(root: &Path, slug: &str, url: &str, generated_at: &str) -> Result<Value, String>` producing spec §8 JSON exactly; `pub const FEED_SCHEMA_VERSION: u32 = 2`.

- [ ] **Step 1: Tests**: from a toy with a hand-written `results.jsonl` (baseline accepted, `a` new accepted, `b` builds-on `a` accepted, `c` rejected at accept, `d` pr valid, `a` re-accepted later): submissions are `baseline, a, b` (a's latest row used), `b.direction == a.direction`, `directions` has `a`'s with root `a`, taxonomy counts, `proposed` flagged, attempts counts, deferred contract present.
- [ ] **Step 2–4:** fail, implement, pass. **Step 5:** Commit `research: feed (schema 2)`.

### Task 11: Research site view

**Files:**
- Modify: `crates/cb/src/pages.rs` (`challenge()` dispatches; research entry = the Task 10 feed plus, per submission, `diff` (bool) and `files`/`readme` for `new`; writes `diffs/<slug>/<id>.patch` via `git diff --no-index --no-color submissions/<parent> submissions/<id>` capped at `MAX_PATCH`), `crates/cb/assets/site/app.js`, `crates/cb/assets/site/style.css`

**Interfaces:** challenges.json entry gains `type: "research"` and the research fields; simple entries unchanged. app.js: if `c.type === "research"` render, in this order: summary/README as today, **trade-off plot** (two `<select>`s, inline SVG scatter, colour per direction from a fixed 8-colour palette that works in light and dark, min–max error bars, baseline as a ring, non-dominated toggle off by default, points are buttons opening the submission panel), **direction tree** (nested list from baseline), **submission panel**, **taxonomy panel**, **deferred contracts**, attempts counts. All text via `textContent`; no external requests.

- [ ] **Step 1: Tests** (Rust): `research_entry_has_map_data`; `builds_on_gets_diff_against_parent` (patch file written, only the two dirs' content); `new_submission_gets_files_not_diff`; `simple_entry_unchanged` (existing tests pass).
- [ ] **Step 2–4:** fail, implement, pass. Then render: `cargo run -q -- feed --root <toy root> --out /tmp/site --base-url http://x --at 2026-11-19T00:00:00Z` and open `/tmp/site/index.html` with the playwright-cli skill; screenshot the research page at desktop and phone width, light and dark; fix anything broken. **Step 5:** Commit `research: site map view`.

### Task 12: Malicious suite, docs

**Files:**
- Create: `scripts/malicious-prs-research.sh` (same structure and `--local` / live modes as `scripts/malicious-prs.sh`, against `challenges/coin-select`; cases: `edit-other-submission`, `edit-judge`, `unknown-parent`, `deferred-contract`, `measure-json`, `symlink`, `gitlink`, `size-cap`, `forged-id`, `direction-taken`, `non-main-base` (ignored), plus live-only `network-in-build` and `read-seed` that pass the gate and must be `rejected` by measurement)
- Modify: `PLAN.md` (research type in decisions and §2–§4), `candidates/README.md`, `.claude/skills/judging-challenge-candidates/SKILL.md` (link a research candidate to its challenge once launched)

- [ ] **Step 1:** Script; `scripts/malicious-prs-research.sh --local` passes every local case (needs `coin-select` status `open` on a scratch branch — the script flips it in its scratch commit, like the existing suite sets up its cases).
- [ ] **Step 2:** Docs. **Step 3:** Commit `research: malicious-PR suite and docs`.
