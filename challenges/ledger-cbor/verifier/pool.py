"""Fetch a pool of mainnet Conway transactions from Koios (public, no key).

  pool.py OUT_PREFIX [--blocks N] [--span-blocks S] [--min-txs M]

Picks N random blocks from the last S blocks before the tip (os.urandom, so
nobody can predict them), writes OUT_PREFIX.hex.gz (one hex tx per line) and
OUT_PREFIX.json (provenance). setup.sh runs it with CB_POOL=fresh for scoring,
so the scored pool is never in the repo. Also how corpus/pool.hex.gz was made.
"""
import argparse, gzip, json, random, sys, time, urllib.request

K = "https://api.koios.rest/api/v1/"
H = {"User-Agent": "cb-ledger-cbor", "Content-Type": "application/json"}
PROTO_MAJOR = 11  # the harness decodes at PV 11; a hard fork needs a new pin


# Koios's public tier sheds load with 429s and 503s, sometimes for minutes
# (PR #84: two scoring runs lost to it in a row). Each call keeps retrying
# with capped exponential backoff and jitter, honouring Retry-After, until
# PATIENCE seconds have passed since its first try.
PATIENCE, MAX_WAIT = 360, 60


def wait_for(e, i):
    """Seconds to wait after failed try i: Retry-After if Koios gives one."""
    after = getattr(e, "headers", None) and e.headers.get("Retry-After")
    if after and after.strip().isdigit():
        return min(int(after), MAX_WAIT * 2)
    return min(3 * 2 ** i, MAX_WAIT) * random.uniform(0.75, 1.25)


def call(path, body=None):
    start, i = time.monotonic(), 0
    while True:
        try:
            data = None if body is None else json.dumps(body).encode()
            req = urllib.request.Request(K + path, data=data, headers=H)
            return json.load(urllib.request.urlopen(req, timeout=60))
        except Exception as e:  # rate limit, outage or transient: back off, then give up
            w = wait_for(e, i)
            if time.monotonic() - start + w > PATIENCE:
                raise RuntimeError(f"koios {path[:80]}: {e} (gave up after {i + 1} tries)")
            print(f"koios {path[:40]}: {e}; retry in {w:.0f}s", file=sys.stderr)
            time.sleep(w)
            i += 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--blocks", type=int, default=250)
    ap.add_argument("--span-blocks", type=int, default=30_000)  # ~7 days
    ap.add_argument("--min-txs", type=int, default=1200)
    a = ap.parse_args()

    tip = call("tip")[0]["block_height"]
    rng = random.SystemRandom()
    heights = sorted(rng.sample(range(tip - 10 - a.span_blocks, tip - 10), a.blocks))
    blocks = []
    for i in range(0, len(heights), 30):
        hs = ",".join(map(str, heights[i : i + 30]))
        blocks += call(f"blocks?block_height=in.({hs})&select=hash,block_height,proto_major,epoch_no")
        time.sleep(0.3)
    other = {b["proto_major"] for b in blocks} - {PROTO_MAJOR}
    if other:
        sys.exit(f"protocol version {sorted(other)} in sampled blocks; harness is pinned to {PROTO_MAJOR}")
    txh = []
    for i in range(0, len(blocks), 20):
        txh += [t["tx_hash"] for t in call("block_txs", {"_block_hashes": [b["hash"] for b in blocks[i : i + 20]]})]
        time.sleep(0.3)
    txs = []
    for i in range(0, len(txh), 50):
        txs += [r["cbor"] for r in call("tx_cbor", {"_tx_hashes": txh[i : i + 50]})]
        time.sleep(0.25)
    if len(txs) < a.min_txs:
        sys.exit(f"only {len(txs)} txs from {len(blocks)} blocks; need {a.min_txs}")
    with gzip.open(a.out + ".hex.gz", "wt") as f:
        f.write("\n".join(txs) + "\n")
    meta = {"source": "api.koios.rest (block_txs, tx_cbor)", "fetched": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "tip": tip, "block_heights": sorted(b["block_height"] for b in blocks),
            "epochs": sorted({b["epoch_no"] for b in blocks}), "proto_major": PROTO_MAJOR,
            "txs": len(txs), "bytes": sum(len(t) // 2 for t in txs)}
    with open(a.out + ".json", "w") as f:
        json.dump(meta, f, indent=1)
    print(f"pool: {len(txs)} txs from {len(blocks)} blocks (heights {heights[0]}-{heights[-1]}, tip {tip})")


if __name__ == "__main__":
    main()
