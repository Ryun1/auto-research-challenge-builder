"""Tag the features of a Cardano transaction's CBOR (curation only; see tricky.py).

  features(tx_bytes) -> set of tags from TAGS

A small standard-library CBOR reader plus a defensive walk over the Conway tx
shape ([body, witnesses, is_valid, aux_data]; Shelley..Mary txs have three
elements). Shapes it doesn't expect are skipped, never raised: the point is to
find txs worth verifying, not to validate them.
"""

import hashlib

BODY_KEYS = {5: "withdrawals", 8: "validity-start", 9: "mint", 14: "required-signers",
             16: "collateral-return", 18: "ref-inputs", 21: "treasury-value", 22: "donation"}
WIT_KEYS = {1: "native-script", 2: "bootstrap-witness", 3: "plutus-v1", 4: "datums",
            5: "redeemers", 6: "plutus-v2", 7: "plutus-v3"}
MANY = 50  # inputs, outputs, or assets in one output

TAGS = tuple(
    [f"gov-proposal:{n}" for n in range(7)] + [f"vote:{n}" for n in range(5)]
    + [f"cert:{n}" for n in range(19)] + list(BODY_KEYS.values()) + list(WIT_KEYS.values())
    + ["legacy-output", "inline-datum", "ref-script", "byron-address", "pointer-address",
       "many-inputs", "many-outputs", "multiasset-heavy", "set-tag-258", "set-no-tag",
       "indefinite", "aux-data", "invalid-tx", "unparsed"])


class Tag:
    def __init__(self, n, v):
        self.n, self.v = n, v


class Map(list):
    """A decoded CBOR map: (key, value) pairs, since keys may be arrays."""

    def get(self, key, default=None):
        return next((v for k, v in self if k == key), default)


class Reader:
    def __init__(self, b):
        self.b, self.i, self.seen = b, 0, set()

    def byte(self):
        if self.i >= len(self.b):
            raise ValueError("truncated")
        self.i += 1
        return self.b[self.i - 1]

    def take(self, n):
        if self.i + n > len(self.b):
            raise ValueError("truncated")
        self.i += n
        return self.b[self.i - n : self.i]

    def arg(self, ai):
        if ai < 24:
            return ai
        if ai in (24, 25, 26, 27):
            return int.from_bytes(self.take(1 << (ai - 24)), "big")
        raise ValueError(f"additional info {ai}")

    def item(self):
        ib = self.byte()
        major, ai = ib >> 5, ib & 31
        if ai == 31 and major in (2, 3, 4, 5):
            self.seen.add("indefinite")
            return self.indefinite(major)
        if major == 7:
            return self.simple(ai)
        n = self.arg(ai)
        if major == 0:
            return n
        if major == 1:
            return -1 - n
        if major in (2, 3):
            raw = self.take(n)
            return raw if major == 2 else raw.decode("utf-8", "replace")
        if major == 4:
            return [self.item() for _ in range(n)]
        if major == 5:
            return Map((self.item(), self.item()) for _ in range(n))
        if n == 258:
            self.seen.add("set-tag-258")
        return Tag(n, self.item())

    def indefinite(self, major):
        out = []
        while self.b[self.i : self.i + 1] != b"\xff":
            out.append(self.item())
            if major == 5:
                out[-1] = (out[-1], self.item())
        self.byte()  # break
        if major == 2:
            return b"".join(out)
        if major == 3:
            return "".join(out)
        return Map(out) if major == 5 else out

    def simple(self, ai):
        if ai in (20, 21):
            return ai == 21
        if ai in (22, 23):
            return None
        if ai in (25, 26, 27):
            self.take(1 << (ai - 24))
            return 0.0
        if ai < 24:
            return ai
        if ai == 24:
            return self.byte()
        raise ValueError(f"simple {ai}")


def decode(b):
    """(value, flags) for exactly one CBOR item, else ValueError."""
    r = Reader(b)
    v = r.item()
    if r.i != len(b):
        raise ValueError("trailing bytes")
    return v, r.seen


def body_hash(tx):
    """The tx id: blake2b-256 of the body's bytes as encoded, or None if unreadable."""
    r = Reader(tx)
    try:
        if r.byte() >> 5 != 4:
            return None
        start = r.i
        r.item()
    except (ValueError, IndexError, RecursionError):
        return None
    return hashlib.blake2b(tx[start : r.i], digest_size=32).hexdigest()


def elems(v):
    """The elements of an array or a tag-258 set; nothing for anything else."""
    if isinstance(v, Tag):
        v = v.v
    return v if isinstance(v, list) and not isinstance(v, Map) else []


def address_tags(addr):
    if isinstance(addr, bytes) and addr:
        if addr[0] >> 4 == 8:
            return {"byron-address"}
        if addr[0] >> 4 in (4, 5):
            return {"pointer-address"}
    return set()


def output_tags(o):
    tags, value = set(), None
    if isinstance(o, Map):
        tags |= address_tags(o.get(0))
        value = o.get(1)
        datum = o.get(2)
        if isinstance(datum, list) and datum[:1] == [1]:
            tags.add("inline-datum")
        if o.get(3) is not None:
            tags.add("ref-script")
    elif isinstance(o, list) and o:
        tags.add("legacy-output")
        tags |= address_tags(o[0])
        value = o[1] if len(o) > 1 else None
    if isinstance(value, list) and len(value) > 1 and isinstance(value[1], Map):
        if sum(len(a) for _, a in value[1] if isinstance(a, Map)) >= MANY:
            tags.add("multiasset-heavy")
    return tags


def body_tags(body):
    tags = {name for k, name in BODY_KEYS.items() if body.get(k) is not None}
    inputs, outputs = body.get(0), elems(body.get(1))
    if isinstance(inputs, list):
        tags.add("set-no-tag")
    if len(elems(inputs)) >= MANY:
        tags.add("many-inputs")
    if len(outputs) >= MANY:
        tags.add("many-outputs")
    for o in outputs:
        tags |= output_tags(o)
    for c in elems(body.get(4)):
        if isinstance(c, list) and c and c[0] in range(19):
            tags.add(f"cert:{c[0]}")
    votes = body.get(19)
    for voter, _ in votes if isinstance(votes, Map) else []:
        if isinstance(voter, list) and voter and voter[0] in range(5):
            tags.add(f"vote:{voter[0]}")
    for p in elems(body.get(20)):
        if isinstance(p, list) and len(p) > 2 and isinstance(p[2], list) and p[2] and p[2][0] in range(7):
            tags.add(f"gov-proposal:{p[2][0]}")
    return tags


def features(tx):
    try:
        v, seen = decode(tx)
    except (ValueError, IndexError, RecursionError):
        return {"unparsed"}
    tags = set(seen)
    if not isinstance(v, list) or isinstance(v, Map) or len(v) < 3:
        return tags
    body, wits = v[0], v[1]
    if isinstance(body, Map):
        tags |= body_tags(body)
    if isinstance(wits, Map):
        tags |= {name for k, name in WIT_KEYS.items() if wits.get(k) is not None}
    valid, aux = (v[2], v[3]) if len(v) > 3 else (True, v[2])
    if valid is False:
        tags.add("invalid-tx")
    if aux is not None:
        tags.add("aux-data")
    return tags
