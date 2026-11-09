"""Headroom and promotion-bar measurements for ledger-cbor (#57). Not part of the judge.

  measure.py selftest EXE FILE              check the Callgrind parser on any `decode FILE REPS` exe
  measure.py collect --out M.json [--pools DIR] [--seeds N] [--pallas EXE]
  measure.py report M.json                  markdown on stdout

collect draws samples exactly as judge.py's score does (SCORE_N txs, seed ^ 0x5C0E)
from the repo pool and from every *.hex.gz in DIR, and for each sample records:
  - the reference's instructions for one decode pass (1-pass minus 0-pass), split by
    the package and module whose code ran them (Callgrind per-instruction costs
    mapped through the binary's symbol table);
  - pallas's instructions for the same sample.

`speedup` is reference/candidate on one sample, so the baseline scores 1.0 on every
pool. What moves between samples is the *mix*: a candidate that removes a fraction f
of the cost of code region G scores 1/(1 - f*share_G(sample)). report turns the
measured share_G spread into the disagreement two promotion runs would see.
"""
import argparse, bisect, collections, concurrent.futures, gzip, itertools, json, os, random, re
import statistics, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "verifier"))
import judge  # SCORE_N, and the same sampling rule

REF = ROOT / ".bin/reference"


# ---- Callgrind -------------------------------------------------------------

def callgrind(exe, args, per_instr):
    """Total Ir, and with per_instr the self cost of every instruction: {(object, addr): Ir}."""
    with tempfile.NamedTemporaryFile(suffix=".cg") as out:
        cmd = ["valgrind", "--tool=callgrind", "--demangle=no", f"--callgrind-out-file={out.name}"]
        if per_instr:
            cmd += ["--dump-instr=yes", "--dump-line=no", "--compress-pos=no", "--compress-strings=no"]
        subprocess.run(cmd + [str(exe)] + args, check=True, capture_output=True, timeout=3600)
        text = Path(out.name).read_text(errors="replace")
    total = int(re.search(r"^(?:summary|totals): (\d+)", text, re.M).group(1))
    return (total, parse(text, total)) if per_instr else (total, None)


def parse(text, total):
    """Self cost per instruction. The line after `calls=` is the callee's inclusive
    cost, not self cost. Returns ({(ob, addr): Ir}, {fn name: lowest addr seen})."""
    cost, fn_min = collections.Counter(), {}
    ob = fn = None
    skip = False
    for line in text.splitlines():
        if line.startswith("0x"):
            if skip:
                skip = False
                continue
            a, _, c = line.partition(" ")
            addr = int(a, 16)
            cost[(ob, addr)] += int(c.split()[0])
            if fn is not None and addr < fn_min.get((ob, fn), 1 << 64):
                fn_min[(ob, fn)] = addr
        elif line.startswith("calls="):
            skip = True
        elif line.startswith("ob="):
            ob = line[3:]
        elif line.startswith("fn="):
            fn = line[3:]
    got = sum(cost.values())
    if got != total:
        raise RuntimeError(f"parser: self costs sum to {got}, Callgrind total is {total}")
    return cost, fn_min


class Symbols:
    """Address -> symbol for one executable, through `nm`. GHC keeps module-internal
    code out of the symbol table, so an address belongs to the nearest symbol before
    it: the right module, because the linker lays each object file out contiguously."""

    def __init__(self, exe):
        out = subprocess.run(["nm", "-n", "--defined-only", str(exe)], check=True,
                             capture_output=True, text=True).stdout
        syms = [(int(a, 16), n) for a, t, n in (l.split(None, 2) for l in out.splitlines() if l.count(" ") >= 2)
                if t in "TtWw"]
        self.addrs, self.names = [a for a, _ in syms], [n for _, n in syms]
        self.by_name = {}
        for a, n in syms:
            self.by_name.setdefault(n, a)
        self.exe, self.ob, self.bias = Path(exe), None, None

    def locate(self, cost, fn_min):
        """Callgrind prints run-time addresses; a PIE is loaded at an offset. Take the
        offset most functions agree on (their first instruction is their symbol)."""
        exe_ob = next(ob for ob in {ob for ob, _ in cost} if Path(ob).name == self.exe.name)
        votes = collections.Counter(a - self.by_name[f] for (ob, f), a in fn_min.items()
                                    if ob == exe_ob and f in self.by_name)
        if not votes:
            raise RuntimeError("no Callgrind function matched a symbol in the executable")
        bias, n = votes.most_common(1)[0]
        if n < 0.5 * sum(votes.values()):
            raise RuntimeError(f"no clear load offset: {votes.most_common(3)}")
        self.ob, self.bias = exe_ob, bias

    def name(self, addr):
        i = bisect.bisect_right(self.addrs, addr - self.bias) - 1
        return self.names[i] if i >= 0 else "?"


ZDEC = {"zm": "-", "zi": ".", "zu": "_", "zz": "z", "ZZ": "Z", "zq": "'", "zh": "#"}


def zdecode(s):
    return re.sub(r"z[miuzqh]|ZZ", lambda m: ZDEC[m.group(0)], s)


def region(sym):
    """(package, module) of a GHC code symbol `<pkg>_<Module>_<name>_info`. RTS and C
    code has no z-encoded module: ('rts+c', symbol)."""
    pkg, _, rest = sym.partition("_")
    mod = rest.partition("_")[0]
    if not (sym.endswith("_info") and mod[:1].isupper() and rest.count("_") >= 2):
        return "rts+c", sym
    return re.sub(r"-\d+(\.\d+)*(-[A-Za-z0-9]+)*$", "", zdecode(pkg)), zdecode(mod)


def split(exe, syms, args):
    """Ir by (package, module) for one run of exe."""
    total, (cost, fn_min) = callgrind(exe, args, True)
    if syms.bias is None:
        syms.locate(cost, fn_min)
    by = collections.Counter()
    for (ob, addr), c in cost.items():
        if ob == syms.ob:
            by[region(syms.name(addr))] += c
        else:
            by[("so:" + re.sub(r"\.so(\.[\d.]+)?$", "", Path(ob).name), "")] += c
    return total, by


def decode_split(exe, syms, sample):
    """One decode pass: the 1-pass run minus the 0-pass run, per region."""
    t1, b1 = split(exe, syms, ["decode", sample, "1"])
    t0, b0 = split(exe, syms, ["decode", sample, "0"])
    by = {k: b1.get(k, 0) - b0.get(k, 0) for k in b1.keys() | b0.keys()}
    return t1 - t0, {f"{p}|{m}": v for (p, m), v in by.items() if v}


def decode_total(exe, mode, sample):
    return callgrind(exe, [mode, sample, "1"], False)[0] - callgrind(exe, [mode, sample, "0"], False)[0]


# ---- collect ---------------------------------------------------------------

def read_pool(path):
    with gzip.open(path, "rt") as f:
        return [bytes.fromhex(l) for l in f.read().split()]


def selftest(exe, path):
    syms = Symbols(exe)
    total, by = split(exe, syms, ["decode", path, "1"])
    top = sorted(by.items(), key=lambda kv: -kv[1])[:5]
    print(f"parser ok: {total} Ir, load offset {syms.bias:#x}, top regions {top}")


def collect(a):
    pools = {"repo": ROOT / "corpus/pool.hex.gz"}
    if a.pools:
        pools.update({p.name.removesuffix(".hex.gz"): p for p in sorted(Path(a.pools).glob("*.hex.gz"))})
    syms = Symbols(REF)
    jobs = []
    for name, path in pools.items():
        txs = read_pool(path)
        meta = path.with_name(path.name.replace(".hex.gz", ".json"))
        info = json.loads(meta.read_text()) if meta.exists() else {}
        for seed in [None] + list(range(1, a.seeds + 1)):
            # seed None: the whole pool, to see what scoring every tx would do.
            s = txs if seed is None else random.Random(seed ^ 0x5C0E).sample(txs, judge.SCORE_N)
            jobs.append({"pool": name, "seed": seed, "n": len(s), "bytes": sum(map(len, s)),
                         "pool_txs": len(txs), "pool_meta": info, "file": judge.write(s)})
    # The first run fixes the load offset; the rest run in parallel.
    def run(j):
        j["ir"], j["regions"] = decode_split(REF, syms, j["file"])
        if a.pallas:
            j["pallas_ir"] = decode_total(a.pallas, "decode", j["file"])
            j["pallas_hash_ir"] = decode_total(a.pallas, "decode-hash", j["file"])
        os.unlink(j.pop("file"))
        print(f"{j['pool']} seed={j['seed']} n={j['n']} ir={j['ir']}", file=sys.stderr, flush=True)
        return j
    done = [run(jobs[0])]
    with concurrent.futures.ThreadPoolExecutor(os.cpu_count() or 2) as ex:
        done += list(ex.map(run, jobs[1:]))
    # Determinism: the first sample again must give the same count.
    again = random.Random(1 ^ 0x5C0E).sample(read_pool(pools["repo"]), judge.SCORE_N)
    f = judge.write(again)
    repeat = decode_split(REF, syms, f)[0]
    first = next(j["ir"] for j in done if j["pool"] == "repo" and j["seed"] == 1)
    Path(a.out).write_text(json.dumps({"score_n": judge.SCORE_N, "repeat_equal": repeat == first,
                                       "samples": done}, indent=1))
    print(f"wrote {a.out}: {len(done)} samples; repeat equal: {repeat == first}", file=sys.stderr)


# ---- report ----------------------------------------------------------------

GAINS = [0.01, 0.05, 0.10, 0.25, 0.50]


def frac_for_gain(shares, gain):
    """f with mean(1/(1 - f*share)) = 1 + gain, or None if removing all of it gives less."""
    mean = lambda f: statistics.fmean(1 / (1 - f * s) for s in shares)
    if max(shares) >= 1 or mean(1.0) < 1 + gain:
        return None
    lo, hi = 0.0, 1.0
    for _ in range(60):
        mid = (lo + hi) / 2
        lo, hi = (mid, hi) if mean(mid) < 1 + gain else (lo, mid)
    return hi


def disagreement(pairs):
    """Relative gaps as cb record computes them: |x - y| / x."""
    d = sorted(abs(x - y) / x for x, y in pairs)
    return d[-1], d[int(0.95 * (len(d) - 1))], d


def report(path, bips):
    m = json.loads(Path(path).read_text())
    S = m["samples"]
    seeded = [s for s in S if s["seed"] is not None]
    whole = [s for s in S if s["seed"] is None]
    pools = sorted({s["pool"] for s in S}, key=lambda p: (p != "repo", p))
    half = bips / 10_000 / 2
    out = []
    w = out.append

    w("## Pools\n")
    w("| pool | txs | bytes/tx | blocks | reference Ir/tx | pallas Ir/tx | pallas+hash Ir/tx | reference / pallas |")
    w("|---|--:|--:|---|--:|--:|--:|--:|")
    for s in whole:
        h = s["pool_meta"].get("block_heights") or s["pool_meta"].get("block_height_range") or []
        blocks = f"{h[0]}-{h[-1]}" if h else "?"
        p, ph = s.get("pallas_ir"), s.get("pallas_hash_ir")
        w(f"| {s['pool']} | {s['n']} | {s['bytes'] / s['n']:.0f} | {blocks} | {s['ir'] / s['n']:,.0f} | "
          + (f"{p / s['n']:,.0f} | {ph / s['n']:,.0f} | {s['ir'] / p:.1f}x ({s['ir'] / ph:.1f}x with hash) |" if p else "- | - | - |"))
    w(f"\nSamples: {len(seeded)} seeded draws of {m['score_n']} txs ({len(seeded) // max(len(pools), 1)} per pool) "
      f"plus each whole pool. Same sample twice gives the same count: **{m['repeat_equal']}**.\n")

    # Where the instructions are. Package-level, then modules of the editable package and cborg.
    def shares(samples, key):
        tot = collections.defaultdict(list)
        keys = {key(r) for s in samples for r in s["regions"]}
        for s in samples:
            agg = collections.Counter()
            for r, v in s["regions"].items():
                agg[key(r)] += v
            for k in keys:
                tot[k].append(agg.get(k, 0) / s["ir"])
        return tot

    pkg = shares(seeded, lambda r: r.split("|")[0])
    w("## Where one decode pass spends its instructions (reference)\n")
    w("Share of Ir by the package whose code ran it, over the seeded samples. GHC inlines and "
      "specialises across packages, so code from `cardano-ledger-binary` that was inlined into an era "
      "instance counts under the era package: read the editable share as a floor, not a ceiling.\n")
    w("| package | mean share | min | max |")
    w("|---|--:|--:|--:|")
    for k, v in sorted(pkg.items(), key=lambda kv: -statistics.fmean(kv[1])):
        if statistics.fmean(v) >= 0.005:
            w(f"| {k} | {statistics.fmean(v):.1%} | {min(v):.1%} | {max(v):.1%} |")
    ed = statistics.fmean(pkg.get("cardano-ledger-binary", [0]))
    cb = statistics.fmean(pkg.get("cborg", [0]))
    w(f"\nIf a candidate removed *all* instructions attributed to `cardano-ledger-binary` ({ed:.1%}), the "
      f"speedup would be {1 / (1 - ed):.2f}x; with `cborg`'s too ({cb:.1%}; frozen, but called from the "
      f"editable code), {1 / (1 - ed - cb):.2f}x.\n")
    mod = shares(seeded, lambda r: r.replace("|", ": "))
    w("Top modules:\n")
    w("| package: module | mean share |")
    w("|---|--:|")
    for k, v in sorted(mod.items(), key=lambda kv: -statistics.fmean(kv[1]))[:15]:
        w(f"| {k} | {statistics.fmean(v):.1%} |")

    # Promotion agreement. Regions = packages with a mean share of 2% or more.
    w("\n## Would two promotion runs agree?\n")
    w(f"`cb record` needs |x - y| / x <= {half:.2%} (half of `min_improvement_bips = {bips}`). A candidate "
      "that removes a fraction of one package's instructions scores differently on each sample, because "
      "that package's share differs. For each package and each mean gain, the table gives the 95th "
      "percentile and the largest gap between two runs, and how often the gap exceeds the limit.\n")
    # promote.yml only ever scores fresh pools; keep the repo pool out of pool-to-pool pairs if we can.
    n_fresh = len([p for p in pools if p != "repo"])
    fresh_only = lambda s: n_fresh < 2 or s["pool"] != "repo"
    kinds = [
        ("same pool, two seeds", lambda v: [(v[i], v[j]) for i, j in itertools.combinations(range(len(seeded)), 2)
                                              if seeded[i]["pool"] == seeded[j]["pool"]]),
        ("two fresh pools (today's promote.yml)", lambda v: [(v[i], v[j]) for i, j in itertools.combinations(range(len(seeded)), 2)
                                                             if seeded[i]["pool"] != seeded[j]["pool"]
                                                             and fresh_only(seeded[i]) and fresh_only(seeded[j])]),
        # The record was set on an older pool than the one a challenger is scored on.
        ("repo pool vs a fresh pool (pools months apart)", lambda v: [(v[i], v[j]) for i, j in itertools.combinations(range(len(seeded)), 2)
                                                                      if (seeded[i]["pool"] == "repo") != (seeded[j]["pool"] == "repo")]),
    ]
    kinds = [(t, f) for t, f in kinds if f(list(range(len(seeded))))]
    worst = {}
    for title, pick in kinds:
        w(f"### {title}\n")
        w("| package (mean share) | " + " | ".join(f"+{g:.0%} gain" for g in GAINS) + " |")
        w("|---|" + "--:|" * len(GAINS))
        for k, v in sorted(pkg.items(), key=lambda kv: -statistics.fmean(kv[1])):
            if statistics.fmean(v) < 0.02:
                continue
            cells = []
            for g in GAINS:
                f = frac_for_gain(v, g)
                if f is None:
                    cells.append("n/a")
                    continue
                mx, p95, d = disagreement(pick([1 / (1 - f * s) for s in v]))
                over = sum(x > half for x in d) / len(d)
                cells.append(f"p95 {p95:.2%}, max {mx:.2%}, over {over:.0%}")
                worst[(title, g)] = max(worst.get((title, g), 0), mx)
            w(f"| {k} ({statistics.fmean(v):.1%}) | " + " | ".join(cells) + " |")
        w("")
    w("Largest gap seen for any package, by gain (n/a cells excluded):\n")
    w("| runs | " + " | ".join(f"+{g:.0%}" for g in GAINS) + " |")
    w("|---|" + "--:|" * len(GAINS))
    for title, _ in kinds:
        w(f"| {title} | " + " | ".join(f"{worst[(title, g)]:.2%}" if (title, g) in worst else "n/a" for g in GAINS) + " |")

    # Scoring every tx in the pool instead of a draw: only pool-to-pool differences remain.
    whole = [s for s in whole if fresh_only(s)]
    if len(whole) > 1:
        wp = shares(whole, lambda r: r.split("|")[0])
        w("\n### Two fresh pools, every tx scored (no draw)\n")
        w("| package | " + " | ".join(f"+{g:.0%} max gap" for g in GAINS) + " |")
        w("|---|" + "--:|" * len(GAINS))
        for k, v in sorted(wp.items(), key=lambda kv: -statistics.fmean(kv[1])):
            if statistics.fmean(v) < 0.02:
                continue
            cells = []
            for g in GAINS:
                f = frac_for_gain(v, g)
                cells.append("n/a" if f is None else
                             f"{disagreement(list(itertools.combinations([1 / (1 - f * s) for s in v], 2)))[0]:.2%}")
            w(f"| {k} | " + " | ".join(cells) + " |")
    print("\n".join(out))


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("selftest"); p.add_argument("exe"); p.add_argument("file")
    p = sub.add_parser("collect")
    p.add_argument("--out", required=True); p.add_argument("--pools"); p.add_argument("--pallas")
    p.add_argument("--seeds", type=int, default=10)
    p = sub.add_parser("report"); p.add_argument("json"); p.add_argument("--bips", type=int, default=100)
    a = ap.parse_args()
    if a.cmd == "selftest":
        selftest(a.exe, a.file)
    elif a.cmd == "collect":
        collect(a)
    else:
        report(a.json, a.bips)
