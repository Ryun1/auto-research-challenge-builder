"""Judge for ledger-cbor: `judge.py verify` and `judge.py score`.

verify checks every tx in the pool (corpus/fresh.hex.gz in CI, else
corpus/pool.hex.gz), 600 CB_SEED mutants of pool txs, the committed hard cases
(corpus/tricky.*, curated by tricky.py) with one seeded mutant each, and 4 edge
cases. score times 800 pool txs drawn with CB_SEED, all of which verify checked.
.bin/reference is the pristine cardano-ledger-binary build, .bin/candidate the submission.
"""
import gzip, json, os, random, re, statistics, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REF, CAND = ROOT / ".bin/reference", ROOT / ".bin/candidate"
MUTANT_N, SCORE_N, WALL_REPS = 600, 800, 5
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


def tricky(path=ROOT / "corpus/tricky.hex.gz"):
    """The committed hard cases. Raises ValueError if the hex and its json disagree."""
    with gzip.open(path, "rt") as f:
        txs = [bytes.fromhex(l) for l in f.read().split()]
    meta = json.loads(path.with_name("tricky.json").read_text())
    if len(txs) != len(meta["txs"]):
        raise ValueError(f"{path.name} has {len(txs)} txs, tricky.json {len(meta['txs'])}")
    return txs


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


def build_cases(seed, p, hard):
    """(kind, input) pairs for verify: every pool tx, seeded pool mutants, the hard
    cases and one seeded mutant of each, then the edge cases. The txs score() times
    are pool txs, so they are among the valid cases."""
    rng = random.Random(seed)
    cases = [("valid", t) for t in p]
    cases += [mutate(rng, t) for t in rng.sample(p, min(MUTANT_N, len(p)))]
    cases += [("tricky", t) for t in hard]
    cases += [("tricky-" + k, m) for k, m in (mutate(rng, t) for t in hard)]
    by_len = sorted(p, key=len)
    return cases + [("edge", b""), ("edge", b"\x00"), ("edge", by_len[0]), ("edge", by_len[-1])]


def verify(seed):
    try:
        hard = tricky()
    except Exception as e:  # any unreadable committed corpus is the judge's fault, not the candidate's
        return {"ok": False, "infra": True, "detail": f"infra: hard-case corpus: {e}"}
    cases = build_cases(seed, pool(), hard)
    path = write([t for _, t in cases])
    ref, cand = lines(REF, "digest", path), lines(CAND, "digest", path)
    # A pool tx is a mainnet tx at PV 11: the reference must accept it. Hard cases
    # include older-era txs it may reject; for those only parity counts.
    if any(not r.startswith("ok ") for (k, _), r in zip(cases, ref) if k == "valid"):
        return {"ok": False, "infra": True, "detail": "infra: reference rejected a mainnet tx"}
    if len(cand) != len(cases):
        return {"ok": False, "detail": f"candidate printed {len(cand)} lines for {len(cases)} inputs"}
    bad = [(i, k, r, c) for i, ((k, _), r, c) in enumerate(zip(cases, ref, cand)) if r != c]
    rejected = sum(not r.startswith("ok ") for r in ref)
    hard_ok = sum(r.startswith("ok ") for (k, _), r in zip(cases, ref) if k == "tricky")
    detail = (f"{pool_label()}: {len(cases)} inputs ({rejected} rejected by reference; whole pool, "
              f"includes the {SCORE_N} scored txs; tricky: {hard_ok} accepted, "
              f"{len(hard) - hard_ok} rejected by reference): {len(bad)} differ")
    if bad:
        detail += "; first: " + ", ".join(f"#{i} {k} ref={r[:12]} cand={c[:12]}" for i, k, r, c in bad[:3])

    # The timed pass forces a decoded tx with rnf, and the candidate owns some
    # NFData instances (Sized), so digests alone don't show that the timed pass
    # decoded everything. On every input the reference accepts, the harness
    # repeats the timed pass's decode + rnf and walks the heap: the candidate
    # may leave no more unevaluated closures and no more function closures than
    # the reference leaves on the same input. That is nothing on almost every
    # mainnet tx, but upstream's own rnf leaves a few thunks on some tx shapes
    # (Promote 43471245947: 'nf 2 0'), and the candidate isn't held to more
    # than the reference does.
    full = [i for i, r in enumerate(ref) if r.startswith("ok ")]
    nf_path = write([cases[i][1] for i in full])
    ref_nf, cand_nf = lines(REF, "nf", nf_path), lines(CAND, "nf", nf_path)
    if len(ref_nf) != len(full) or any(nf(r) is None for r in ref_nf):
        odd = next((r for r in ref_nf if nf(r) is None), f"{len(ref_nf)} lines")
        return {"ok": False, "infra": True, "detail": f"infra: no heap walk of the reference ({odd!r})"}
    if len(cand_nf) != len(full):
        return {"ok": False, "detail": f"candidate printed {len(cand_nf)} nf lines for {len(full)} inputs"}
    lazy = [(i, r, c) for i, r, c in zip(full, ref_nf, cand_nf)
            if nf(c) is None or nf(c)[0] > nf(r)[0] or nf(c)[1] > nf(r)[1]]
    ref_lazy = [(i, r) for i, r in zip(full, ref_nf) if nf(r)[0]]
    if ref_lazy:
        detail += f"; reference left thunks on {len(ref_lazy)}: " + ", ".join(
            f"#{i} {cases[i][0]} {len(cases[i][1])}B {r!r}" for i, r in ref_lazy[:3])
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
