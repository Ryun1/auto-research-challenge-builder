"""Fetch the mainnet fixtures that check the byte model (dev-time; needs network).

  python3 challenges/coin-select/contracts/wallet-multiasset/fixtures/fetch.py [--max-txs N]

Walks back from the Koios tip (blocks, block_txs, tx_cbor), keeps transactions
the byte model is meant to cover (see README.md for the filter), and stops once
the kept set covers every case test_fixtures.py needs. Datum-hash outputs no
longer appear on recent mainnet, so a second walk forward from epoch 400 takes
the first few txs that have one. Writes mainnet.jsonl next to this file: a meta
header line (epoch params, excluded counts, coverage), then one transaction per
line with a params_match flag. Not imported by the judge.
"""
import argparse, json, os, random, sys, time, urllib.error, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from cbor import NonCanonical, decode  # noqa: E402
from model import PARAMS, min_fee  # noqa: E402
from test_fixtures import form_of, outputs, parse_output  # noqa: E402

K = "https://api.koios.rest/api/v1/"
H = {"User-Agent": "cb-coin-select", "Content-Type": "application/json"}
OUT = os.path.join(HERE, "mainnet.jsonl")

BODY_KEYS = {0, 1, 2, 3, 7, 8, 15}
KEEP = 60
EXACT_FEE_TXS = 20
WITNESS_COUNTS = 3
MANY_ASSETS = 20

# Koios's public tier sheds load with 429s and 503s; back off and retry those,
# but never retry another 4xx: that is the query's fault (see pool.py).
PATIENCE, MAX_WAIT = 360, 60


def call(path, body=None):
    start, i = time.monotonic(), 0
    while True:
        try:
            data = None if body is None else json.dumps(body).encode()
            req = urllib.request.Request(K + path, data=data, headers=H)
            return json.load(urllib.request.urlopen(req, timeout=60))
        except urllib.error.HTTPError as e:
            if 400 <= e.code < 500 and e.code != 429:
                raise RuntimeError(f"koios {path[:80]}: HTTP {e.code} {e.read()[:200]!r}")
            err = e
        except Exception as e:  # outage or transient
            err = e
        after = getattr(err, "headers", None) and err.headers.get("Retry-After")
        w = min(int(after), MAX_WAIT * 2) if after and after.strip().isdigit() \
            else min(3 * 2 ** i, MAX_WAIT) * random.uniform(0.75, 1.25)
        if time.monotonic() - start + w > PATIENCE:
            raise RuntimeError(f"koios {path[:80]}: {err} (gave up after {i + 1} tries)")
        print(f"koios {path[:40]}: {err}; retry in {w:.0f}s", file=sys.stderr)
        time.sleep(w)
        i += 1


def classify(cbor_hex):
    """None if the tx is kept, else the reason it is excluded."""
    data = bytes.fromhex(cbor_hex)
    try:
        tx, end = decode(data, 0)
    except NonCanonical as e:
        msg = str(e)
        return "indefinite" if "indefinite" in msg else "non_minimal" if "non-minimal" in msg else "duplicate_key"
    except ValueError:
        return "undecodable"
    except TypeError:  # a map keyed by an array or map (metadata, redeemers): cbor.decode can't hold it
        return "unhashable_key"
    if end != len(data) or not isinstance(tx, list) or len(tx) != 4:
        return "undecodable"
    body, wits, valid, _ = tx
    if not set(body) <= BODY_KEYS:
        return "out_of_scope"
    if set(wits) != {0}:
        return "witness_keys"
    if valid is not True:
        return "invalid"
    outs = body[1]
    if any(isinstance(o, dict) and 3 in o for o in outs):
        return "script_ref"
    if any(not isinstance(o[1], int) and not o[1][1] for o in outs):
        return "empty_multiasset"  # [coin, {}]: legal, but model.Value can't tell it from a bare coin
    if not any(_has_assets(o) for o in outs):
        return "no_assets"
    return None


def _has_assets(o):
    val = o[1]  # value is item/key 1 in both output forms
    return not isinstance(val, int) and bool(val[1])


def features(f):
    """The coverage cases one kept tx contributes."""
    tx, _ = decode(bytes.fromhex(f["cbor"]), 0)
    body, wits = tx[0], tx[1]
    got = {"tagged" if hasattr(body[0], "tag") else "untagged"}
    if f["exact_min_fee"]:
        got.add("exact_min_fee")
    for orig, out in outputs(f):
        got.add(form_of(orig))
        if out.datum:
            got.add("datum_" + out.datum[0])
        if sum(len(n) for n in out.value[1].values()) >= MANY_ASSETS:
            got.add("many_assets")
        if out.value[0] == (160 + len(orig)) * PARAMS["coins_per_utxo_byte"]:
            got.add("exact_min_ada")
    w = wits[0]
    n = len(w.value if hasattr(w, "tag") else w)
    return got, n


# Cases the recent walk must cover. Datum-hash outputs are gone from recent
# mainnet (none in 20,048 txs), so a second walk from DATUM_HASH_FROM finds them.
SINGLE = ["exact_min_ada", "many_assets", "datum_inline", "map", "array", "tagged", "untagged"]
DATUM_HASH_TXS = 3
DATUM_HASH_FROM = 8_526_132  # first block of epoch 400 (Babbage; same params as model.PARAMS)


def coverage(rows):
    feats = [features(r) for r in rows]
    cov = {k: sum(k in g for g, _ in feats) for k in SINGLE + ["datum_hash", "exact_min_fee"]}
    cov["witness_counts"] = sorted({n for _, n in feats})
    return cov


def covered(cov):
    return (all(cov[k] >= 1 for k in SINGLE) and cov["exact_min_fee"] >= EXACT_FEE_TXS
            and len(cov["witness_counts"]) >= WITNESS_COUNTS)


def select(pool, keep):
    """keep rows from pool covering every recent-walk case, in scan order."""
    feats = [features(r) for r in pool]
    chosen = set()

    def have(k):
        return sum(k in feats[i][0] for i in chosen)

    for k in SINGLE:
        if not have(k):
            chosen.add(next(i for i, (g, _) in enumerate(feats) if k in g))
    for i, (_, n) in enumerate(feats):
        if len({feats[j][1] for j in chosen}) >= WITNESS_COUNTS:
            break
        if n not in {feats[j][1] for j in chosen}:
            chosen.add(i)
    for i, (g, _) in enumerate(feats):
        if have("exact_min_fee") >= EXACT_FEE_TXS:
            break
        if "exact_min_fee" in g:
            chosen.add(i)
    for i in range(len(pool)):
        if len(chosen) >= keep:
            break
        chosen.add(i)
    return [pool[i] for i in sorted(chosen)]


def epoch_params(epoch):
    p = call(f"epoch_params?_epoch_no={epoch}&select=epoch_no,min_fee_a,min_fee_b,"
             "coins_per_utxo_size,max_tx_size,max_val_size,protocol_major")[0]
    got = {"min_fee_a": p["min_fee_a"], "min_fee_b": p["min_fee_b"],
           "coins_per_utxo_byte": int(p["coins_per_utxo_size"]),
           "max_tx_size": p["max_tx_size"], "max_val_size": int(p["max_val_size"])}
    return dict(got, epoch_no=p["epoch_no"], protocol_major=p["protocol_major"]), \
        got == {k: PARAMS[k] for k in got}


def row(r):
    """A fixture row for an eligible tx. The ledger's fee size leaves out the
    is_valid byte (see model.tx_size), hence len - 1."""
    tx, _ = decode(bytes.fromhex(r["cbor"]), 0)
    fee = tx[0][2]
    return {"hash": r["tx_hash"], "cbor": r["cbor"], "epoch": r["epoch_no"], "fee": fee,
            "exact_min_fee": fee == min_fee(len(r["cbor"]) // 2 - 1)}


def walk(cursor, newest_first, max_txs, wanted, done):
    """Scan blocks from cursor; keep eligible txs for which wanted(row) holds
    until done(kept). Returns (kept, excluded counts, scanned, height range)."""
    kept, excluded, scanned, heights = [], {}, 0, []
    cmp, order = ("lt", "desc") if newest_first else ("gt", "asc")
    while scanned < max_txs:
        blocks = call(f"blocks?select=hash,block_height,epoch_no,tx_count&block_height={cmp}.{cursor}"
                      f"&tx_count=gt.0&order=block_height.{order}&limit=40")
        if not blocks:
            break
        cursor = blocks[-1]["block_height"]
        heights += [b["block_height"] for b in blocks]
        txh = []
        for i in range(0, len(blocks), 20):
            txh += [t["tx_hash"] for t in call("block_txs", {"_block_hashes": [b["hash"] for b in blocks[i:i + 20]]})]
        for i in range(0, len(txh), 50):
            for r in call("tx_cbor", {"_tx_hashes": txh[i:i + 50]}):
                scanned += 1
                why = classify(r["cbor"])
                if why:
                    excluded[why] = excluded.get(why, 0) + 1
                elif wanted(row(r)):
                    kept.append(row(r))
            time.sleep(0.25)
        print(f"scanned {scanned} txs, kept {len(kept)}; coverage {coverage(kept)}", file=sys.stderr)
        if done(kept):
            break
    return kept, dict(sorted(excluded.items())), scanned, [min(heights), max(heights)] if heights else []


def has_datum_hash(r):
    return "datum_hash" in features(r)[0]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--max-txs", type=int, default=20_000, help="give up a walk after scanning this many txs")
    ap.add_argument("--partial", help="if coverage is not reached, write the eligible txs here (for diagnosis)")
    a = ap.parse_args()

    tip = call("tip")[0]["block_height"]
    pool, excluded, scanned, heights = walk(
        tip - 10, True, a.max_txs, lambda r: True,  # tip - 10: stay clear of rollbacks
        lambda kept: len(kept) >= KEEP and covered(coverage(kept)))
    cov = coverage(pool)
    if not covered(cov) or len(pool) < KEEP:
        if a.partial:
            with open(a.partial, "w") as f:
                f.write(json.dumps({"type": "meta", "partial": True, "scanned": scanned,
                                    "excluded": excluded, "coverage": cov}) + "\n")
                f.writelines(json.dumps(r) + "\n" for r in pool)
        sys.exit(f"coverage not reached after {scanned} txs: kept {len(pool)}, {cov}, excluded {excluded}")
    dh, dh_excluded, dh_scanned, dh_heights = walk(
        DATUM_HASH_FROM, False, a.max_txs, has_datum_hash, lambda kept: len(kept) >= DATUM_HASH_TXS)
    if len(dh) < DATUM_HASH_TXS:
        sys.exit(f"only {len(dh)} datum-hash txs after {dh_scanned} txs from height {DATUM_HASH_FROM}")
    dh = dh[:DATUM_HASH_TXS]

    rows = select(pool, KEEP - len(dh)) + dh
    params = {e: epoch_params(e) for e in sorted({r["epoch"] for r in rows})}
    for r in rows:
        r["params_match"] = params[r["epoch"]][1]
    latest, ok = params[max(params)]
    if not ok:
        sys.exit(f"epoch {latest['epoch_no']} params {latest} differ from model.PARAMS")
    meta = {"type": "meta", "source": "api.koios.rest (blocks, block_txs, tx_cbor, epoch_params)",
            "fetched": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "tip": tip,
            "params": latest, "epochs": {e: ok for e, (_, ok) in params.items()}, "kept": len(rows),
            "recent": {"block_heights": heights, "scanned": scanned, "eligible": len(pool)},
            "datum_hash_walk": {"block_heights": dh_heights, "scanned": dh_scanned, "excluded": dh_excluded},
            "excluded": excluded, "coverage": coverage(rows)}
    with open(OUT, "w") as f:
        f.write(json.dumps(meta) + "\n")
        for r in rows:
            f.write(json.dumps(r) + "\n")
    print(f"wrote {len(rows)} txs to {OUT}")
    print(f"coverage {meta['coverage']}")
    print(f"excluded {excluded} of {scanned} recent txs ({len(pool)} eligible)")
    print(f"datum-hash walk: {dh_scanned} txs from height {DATUM_HASH_FROM}, excluded {dh_excluded}")


if __name__ == "__main__":
    main()
