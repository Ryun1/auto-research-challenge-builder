"""Curate corpus/tricky.*: mainnet txs with rare shapes, verified on every run.

  python3 verifier/tricky.py [--cap 200] [--per-tag 5] [--largest 20]

Run by hand (Koios, public tier), and the output is committed: judge.py never
fetches it. Candidates come from the repo pool (corpus/pool.hex.gz), every governance
proposal, votes by each voter role, DRep and pool certificate txs, Plutus V1/V2
script creations, the largest
recent blocks and random blocks over the last ~4 years (older protocol versions). cborscan.features tags
each candidate and select() keeps the rarest tags first. Tags no candidate
carries are listed as missing_tags in tricky.json.
"""
import argparse, collections, gzip, json, random, sys, time
from pathlib import Path

from cborscan import TAGS, body_hash, features
from pool import call

ROOT = Path(__file__).resolve().parent.parent
VOTER_ROLES = ("ConstitutionalCommittee", "DRep", "SPO")
SCRIPT_TYPES = ("plutusV1", "plutusV2")  # Koios lists no plutusV3 scripts; V3 comes from blocks
SPAN_BLOCKS = 4 * 365 * 4320  # ~4 years of blocks, back into Babbage


def select(cands, per_tag=5, largest=20, cap=200):
    """Hashes to keep from {hash: {"bytes", "tags", "pv"}}, deterministically.

    Buckets are tags plus "pv:<n>". Pass 1 takes one tx per bucket, rarest bucket
    first, so every bucket is represented while the cap allows; pass 2 fills each
    bucket up to per_tag; pass 3 adds the largest txs overall. Within a bucket the
    largest tx goes first, ties by hash.
    """
    buckets = collections.defaultdict(list)
    for h, c in cands.items():
        for t in c["tags"]:
            buckets[t].append(h)
        if c.get("pv") is not None:
            buckets[f"pv:{c['pv']}"].append(h)
    by_size = lambda h: (-cands[h]["bytes"], h)
    order = sorted(buckets, key=lambda b: (len(buckets[b]), b))
    chosen, have = [], collections.Counter()

    def take(h):
        if h in chosen or len(chosen) >= cap:
            return
        chosen.append(h)
        c = cands[h]
        have.update(c["tags"] | ({f"pv:{c['pv']}"} if c.get("pv") is not None else set()))

    for quota in (1, per_tag):
        for b in order:
            for h in sorted(buckets[b], key=by_size):
                if have[b] >= quota:
                    break
                take(h)
    for h in sorted(cands, key=by_size)[:largest]:
        take(h)
    return chosen


def block_txs(blocks):
    """{tx_hash: pv} for the txs in these blocks ({hash, proto_major} rows)."""
    out = {}
    for i in range(0, len(blocks), 20):
        chunk = blocks[i : i + 20]
        pv = {b["hash"]: b.get("proto_major") for b in chunk}
        for t in call("block_txs", {"_block_hashes": list(pv)}):
            out[t["tx_hash"]] = pv.get(t["block_hash"])
    return out


def source(name, fn):
    """A failing source is reported and skipped, so one bad endpoint doesn't lose the run."""
    try:
        got = fn()
    except Exception as e:  # noqa: BLE001 - curation tool, keep going
        print(f"source {name}: skipped ({e})", file=sys.stderr)
        return {}
    print(f"source {name}: {len(got)} txs", file=sys.stderr)
    return got


def gather(random_blocks=150, big_blocks=30):
    """{tx_hash: pv or None} from every candidate source."""
    hashes = {}
    hashes |= source("proposals", lambda: {r["proposal_tx_hash"]: None for r in
                                            call("proposal_list?select=proposal_tx_hash")})
    for role in VOTER_ROLES:
        hashes |= source(f"votes:{role}", lambda role=role: {r["vote_tx_hash"]: None for r in call(
            f"vote_list?voter_role=eq.{role}&select=vote_tx_hash,block_time&order=block_time.desc&limit=100")})
    for kind in SCRIPT_TYPES:
        hashes |= source(f"scripts:{kind}", lambda kind=kind: {r["creation_tx_hash"]: None for r in call(
            f"plutus_script_list?type=eq.{kind}&select=creation_tx_hash&limit=50")})
    for action in ("registered", "updated", "deregistered"):
        hashes |= source(f"drep:{action}", lambda action=action: {r["update_tx_hash"]: None for r in call(
            f"drep_updates?action=eq.{action}&select=update_tx_hash,block_time&order=block_time.desc&limit=50")})
    hashes |= source("pool-updates", lambda: {r["tx_hash"]: None for r in call(
        "pool_updates?select=tx_hash,block_time,retiring_epoch&order=block_time.desc&limit=100")})
    hashes |= source("pool-retirements", lambda: {r["tx_hash"]: None for r in call(
        "pool_updates?retiring_epoch=not.is.null&select=tx_hash,block_time&order=block_time.desc&limit=50")})
    hashes |= source("big-blocks", lambda: block_txs(call(
        f"blocks?block_size=gt.80000&block_height=gt.0&select=hash,block_height,proto_major"
        f"&order=block_height.desc&limit={big_blocks}")))

    def random_sample():
        tip = call("tip")[0]["block_height"]
        heights = sorted(random.SystemRandom().sample(range(tip - SPAN_BLOCKS, tip - 10), random_blocks))
        blocks = []
        for i in range(0, len(heights), 30):
            hs = ",".join(map(str, heights[i : i + 30]))
            blocks += call(f"blocks?block_height=in.({hs})&select=hash,proto_major")
        return block_txs(blocks)

    random_txs = source("random-blocks", random_sample)
    for h, pv in random_txs.items():  # a random-block tx keeps its protocol version
        hashes[h] = pv
    return hashes


def fetch(hashes):
    """{tx_hash: {"cbor": bytes, "block_height": int|None}} via tx_cbor, 50 per call."""
    out, hs = {}, sorted(hashes)
    for i in range(0, len(hs), 50):
        for r in call("tx_cbor", {"_tx_hashes": hs[i : i + 50]}):
            out[r["tx_hash"]] = {"cbor": bytes.fromhex(r["cbor"]), "block_height": r.get("block_height")}
        time.sleep(0.25)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cap", type=int, default=200)
    ap.add_argument("--per-tag", type=int, default=5)
    ap.add_argument("--largest", type=int, default=20)
    a = ap.parse_args()

    pvs = gather()
    txs = fetch(pvs)
    with gzip.open(ROOT / "corpus/pool.hex.gz", "rt") as f:  # PV 11 mainnet txs, no calls needed
        for line in f.read().split():
            cbor = bytes.fromhex(line)
            h = body_hash(cbor)
            if h and h not in txs:
                txs[h], pvs[h] = {"cbor": cbor, "block_height": None}, 11
    cands = {h: {"bytes": len(t["cbor"]), "tags": features(t["cbor"]), "pv": pvs.get(h)} for h, t in txs.items()}
    keep = select(cands, a.per_tag, a.largest, a.cap)

    counts = collections.Counter(t for c in cands.values() for t in c["tags"])
    kept = collections.Counter(t for h in keep for t in cands[h]["tags"])
    missing = [t for t in TAGS if t != "unparsed" and not counts[t]]
    for t in TAGS:
        print(f"{t:22} candidates {counts[t]:6}  kept {kept[t]:4}", file=sys.stderr)

    with open(ROOT / "corpus/tricky.hex.gz", "wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", mtime=0) as f:
        f.write("".join(txs[h]["cbor"].hex() + "\n" for h in keep).encode())
    meta = {"source": "corpus/pool.hex.gz + api.koios.rest (proposal_list, vote_list, drep_updates, pool_updates, plutus_script_list, blocks, block_txs, tx_cbor)",
            "fetched": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "candidates": len(cands),
            "txs": [{"hash": h, "bytes": cands[h]["bytes"], "block_height": txs[h]["block_height"],
                     "pv": cands[h]["pv"], "tags": sorted(cands[h]["tags"])} for h in keep],
            "missing_tags": missing}
    (ROOT / "corpus/tricky.json").write_text(json.dumps(meta, indent=1) + "\n")
    print(f"tricky: kept {len(keep)} of {len(cands)} candidates "
          f"({sum(cands[h]['bytes'] for h in keep)} bytes); missing tags: {missing}", file=sys.stderr)


if __name__ == "__main__":
    main()
