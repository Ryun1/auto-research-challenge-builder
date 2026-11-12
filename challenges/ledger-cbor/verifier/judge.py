"""Judge for ledger-cbor: `judge.py verify` and `judge.py score`.

Inputs are drawn from corpus/pool.hex.gz with CB_SEED, after the build.
.bin/reference is the pristine cardano-ledger-binary build, .bin/candidate the submission.
"""
import gzip, json, os, random, re, statistics, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REF, CAND = ROOT / ".bin/reference", ROOT / ".bin/candidate"
VALID_N, MUTANT_N, SCORE_N, WALL_REPS = 600, 300, 800, 5
WALL_ATTEMPTS = 3  # the paired wall-time measurement is repeated only while it is unstable


def pool_path():
    # setup.sh writes corpus/fresh.* when CB_POOL=fresh (CI scoring): recent
    # random mainnet blocks, never in the repo. Otherwise the repo pool.
    fresh = ROOT / "corpus/fresh.hex.gz"
    return fresh if fresh.exists() else ROOT / "corpus/pool.hex.gz"


def pool_label():
    """Which pool, and for a fresh one where it came from (for the score record)."""
    p = pool_path()
    meta = p.with_name(p.name.replace(".hex.gz", ".json"))
    if p.name.startswith("fresh") and meta.exists():
        m = json.loads(meta.read_text())
        h = m["block_heights"]
        return f"fresh pool ({m['txs']} txs, {len(h)} blocks {h[0]}-{h[-1]}, tip {m['tip']}, {m['fetched']})"
    return "repo pool (corpus/pool.hex.gz)"


def pool():
    with gzip.open(pool_path(), "rt") as f:
        return [bytes.fromhex(l) for l in f.read().split()]


def write(txs):
    f = tempfile.NamedTemporaryFile("w", suffix=".hex", delete=False)
    f.write("".join(t.hex() + "\n" for t in txs))
    f.close()
    return f.name


def mutate(rng, tx):
    kind = rng.choice(["truncate", "flip", "append", "prefix"])
    if kind == "truncate":
        return kind, tx[: rng.randrange(1, len(tx))]
    if kind == "flip":
        i = rng.randrange(len(tx))
        return kind, tx[:i] + bytes([tx[i] ^ rng.randrange(1, 256)]) + tx[i + 1 :]
    if kind == "append":
        return kind, tx + bytes([rng.randrange(256)])
    return kind, bytes([rng.choice([0x80, 0xA0, 0xD8])]) + tx


def lines(exe, mode, path):
    """One output line per input from `harness digest|nf`."""
    out = subprocess.run([str(exe), mode, path], capture_output=True, text=True, timeout=900)
    if out.returncode != 0:
        raise RuntimeError(f"{exe.name} {mode} exit {out.returncode}: {out.stderr[-300:]}")
    return out.stdout.splitlines()


def score_sample(seed, p):
    """The txs score() times for this seed. verify() checks these same txs."""
    return random.Random(seed ^ 0x5C0E).sample(p, SCORE_N)


def nf(line):
    """(unevaluated, functions) from an `nf` line, or None for anything else."""
    m = re.fullmatch(r"nf (\d+) (\d+)", line)
    return (int(m.group(1)), int(m.group(2))) if m else None


def verify(seed):
    rng, p = random.Random(seed), pool()
    cases = [("valid", t) for t in rng.sample(p, VALID_N)]
    cases += [mutate(rng, t) for t in rng.sample(p, MUTANT_N)]
    by_len = sorted(p, key=len)
    cases += [("edge", b""), ("edge", b"\x00"), ("edge", by_len[0]), ("edge", by_len[-1])]
    # The txs score() will time with this seed: their digests are checked too, so
    # the timed inputs are known to decode to the reference's values.
    cases += [("scored", t) for t in score_sample(seed, p)]
    path = write([t for _, t in cases])
    ref, cand = lines(REF, "digest", path), lines(CAND, "digest", path)
    if any(not r.startswith("ok ") for (k, _), r in zip(cases, ref) if k in ("valid", "scored")):
        return {"ok": False, "detail": "infra: reference rejected a mainnet tx"}
    if len(cand) != len(cases):
        return {"ok": False, "detail": f"candidate printed {len(cand)} lines for {len(cases)} inputs"}
    bad = [(i, k, r, c) for i, ((k, _), r, c) in enumerate(zip(cases, ref, cand)) if r != c]
    rejected = sum(not r.startswith("ok ") for r in ref)
    detail = f"{pool_label()}: {len(cases)} inputs ({rejected} rejected by reference): {len(bad)} differ"
    if bad:
        detail += "; first: " + ", ".join(f"#{i} {k} ref={r[:12]} cand={c[:12]}" for i, k, r, c in bad[:3])

    # The timed pass forces a decoded tx with rnf, and the candidate owns some
    # NFData instances (Sized), so digests alone don't show that the timed pass
    # decoded everything. On every input the reference accepts, the harness
    # repeats the timed pass's decode + rnf and walks the heap: nothing may be
    # left unevaluated, and no more function closures than the reference has.
    full = [i for i, r in enumerate(ref) if r.startswith("ok ")]
    nf_path = write([cases[i][1] for i in full])
    ref_nf, cand_nf = lines(REF, "nf", nf_path), lines(CAND, "nf", nf_path)
    if len(ref_nf) != len(full) or any(nf(r) is None or nf(r)[0] for r in ref_nf):
        odd = next((r for r in ref_nf if nf(r) is None or nf(r)[0]), f"{len(ref_nf)} lines")
        return {"ok": False, "detail": f"infra: reference not fully evaluated after rnf ({odd!r})"}
    if len(cand_nf) != len(full):
        return {"ok": False, "detail": f"candidate printed {len(cand_nf)} nf lines for {len(full)} inputs"}
    lazy = [(i, r, c) for i, r, c in zip(full, ref_nf, cand_nf)
            if nf(c) is None or nf(c)[0] or nf(c)[1] > nf(r)[1]]
    detail += f"; {len(lazy)} of {len(full)} not fully evaluated by the timed pass's rnf"
    if lazy:
        detail += "; first: " + ", ".join(f"#{i} {cases[i][0]} ref={r!r} cand={c[:20]!r}" for i, r, c in lazy[:3])
    return {"ok": not bad and not lazy, "detail": detail}


def instructions(exe, path, reps):
    with tempfile.NamedTemporaryFile(suffix=".cg") as out:
        subprocess.run(["valgrind", "--tool=callgrind", f"--callgrind-out-file={out.name}",
                        str(exe), "decode", path, str(reps)], check=True, capture_output=True, timeout=1800)
        m = re.search(rb"^(?:summary|totals): (\d+)", Path(out.name).read_bytes(), re.M)
        return int(m.group(1))


def rel_mad(xs):
    m = statistics.median(xs)
    return statistics.median(abs(x - m) for x in xs) / m


def score(seed):
    sample = write(score_sample(seed, pool()))
    # Instructions for decoding the sample once; start-up and reading the hex
    # input (the 0-pass run) subtracted.
    ic = instructions(CAND, sample, 1) - instructions(CAND, sample, 0)
    ir = instructions(REF, sample, 1) - instructions(REF, sample, 0)
    blocks = int(os.environ.get("CB_PERF_BLOCKS", 4))
    max_disp = float(os.environ.get("CB_PERF_MAX_DISPERSION", 0.08))

    def time(cand):
        exe = CAND if cand else REF
        out = subprocess.run([str(exe), "decode", sample, str(WALL_REPS)], check=True,
                             capture_output=True, text=True, timeout=900)
        return float(out.stdout.split()[-1])

    def measure():
        all_b, all_c, ratios = [], [], []
        for k in range(blocks):  # mirrored BCCB / CBBC, as kits/rust
            order = [False, True, True, False] if k % 4 in (0, 3) else [True, False, False, True]
            b, c = [], []
            for cand in order:
                (c if cand else b).append(time(cand))
            ratios.append(statistics.median(c) / statistics.median(b))
            all_b += b
            all_c += c
        g = statistics.geometric_mean(ratios)
        cross = max(abs(r / g - 1) for r in ratios)
        mad_c, mad_b = rel_mad(all_c), rel_mad(all_b)
        return mad_c <= max_disp and mad_b <= max_disp and cross <= max_disp, 1 / g, mad_c, mad_b, cross

    time(True), time(False)  # warm up
    # A burst of load on the shared runner spoils one measurement (Promote
    # 43135597462: cross-block 0.22 on one of the two runs, 0.02 on the other).
    # Only an unstable measurement is repeated. The first stable one decides,
    # pass or regressed, so a regression can't be retried away.
    for attempts in range(1, WALL_ATTEMPTS + 1):
        stable, wall, mad_c, mad_b, cross = measure()
        if stable:
            break
    guard = "unstable" if not stable else "regressed" if wall < 1 - max_disp else "pass"
    return {"metrics": {
        "speedup": ir / ic, "instructions": ic, "reference_instructions": ir,
        "wall_speedup": round(wall, 4), "wall_rel_mad_candidate": round(mad_c, 4),
        "wall_rel_mad_reference": round(mad_b, 4), "wall_cross_block": round(cross, 4),
        "wall_attempts": attempts, "guard": guard, "n": SCORE_N}}


if __name__ == "__main__":
    seed = int(os.environ["CB_SEED"])
    if sys.argv[1] == "verify":
        try:
            r = verify(seed)
        except Exception as e:  # a crash or timeout is a failed verify, not a pass
            r = {"ok": False, "detail": str(e)[-400:]}
        print(json.dumps(r))
        sys.exit(0 if r["ok"] else 1)
    print(json.dumps(score(seed)))
