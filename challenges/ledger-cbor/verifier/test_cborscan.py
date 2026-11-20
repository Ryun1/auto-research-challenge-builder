"""Tests for cborscan.features: python3 -m unittest discover -s verifier -p 'test_*.py'"""
import unittest

import hashlib

from cborscan import TAGS, body_hash, features


class T:
    """A CBOR tag, for enc()."""
    def __init__(self, n, v):
        self.n, self.v = n, v


class Indef(list):
    """A list enc() writes with indefinite length."""


class M(list):
    """A CBOR map as (key, value) pairs, for keys Python can't hash (arrays)."""


def head(major, n):
    if n < 24:
        return bytes([major << 5 | n])
    for ai, size in ((24, 1), (25, 2), (26, 4), (27, 8)):
        if n < 1 << (8 * size):
            return bytes([major << 5 | ai]) + n.to_bytes(size, "big")
    raise ValueError(n)


def enc(v):
    """Hand-written CBOR encoder for test inputs (no library)."""
    if v is None:
        return b"\xf6"
    if v is True:
        return b"\xf5"
    if v is False:
        return b"\xf4"
    if isinstance(v, int):
        return head(0, v) if v >= 0 else head(1, -1 - v)
    if isinstance(v, bytes):
        return head(2, len(v)) + v
    if isinstance(v, str):
        return head(3, len(v.encode())) + v.encode()
    if isinstance(v, M):
        return head(5, len(v)) + b"".join(enc(k) + enc(x) for k, x in v)
    if isinstance(v, Indef):
        return b"\x9f" + b"".join(map(enc, v)) + b"\xff"
    if isinstance(v, list):
        return head(4, len(v)) + b"".join(map(enc, v))
    if isinstance(v, dict):
        return head(5, len(v)) + b"".join(enc(k) + enc(x) for k, x in v.items())
    if isinstance(v, T):
        return head(6, v.n) + enc(v.v)
    raise TypeError(v)


H32 = bytes(32)
TXIN = [H32, 0]
SHELLEY_ADDR = b"\x61" + bytes(28)  # enterprise key-hash address
BYRON_ADDR = b"\x82\xd8\x18" + bytes(10)


def tx(body, wits=None, valid=True, aux=None):
    return enc([body, wits or {}, valid, aux])


def body(**extra):
    b = {0: [TXIN], 1: [[SHELLEY_ADDR, 1_000_000]], 2: 170_000}
    b.update({int(k[1:]): v for k, v in extra.items()})
    return b


class Features(unittest.TestCase):
    def test_minimal_tx(self):
        self.assertEqual(features(tx({0: [], 1: [], 2: 0})), {"set-no-tag"})

    def test_body_keys(self):
        f = features(tx(body(k5={b"\xe1" + bytes(28): 5}, k9={bytes(28): {b"x": 1}},
                             k18=[TXIN], k22=10)))
        self.assertTrue({"withdrawals", "mint", "ref-inputs", "donation"} <= f, f)

    def test_governance(self):
        proposal = [100_000_000_000, b"\xe1" + bytes(28), [6], ["https://x", H32]]
        # voting_procedures = { voter => { gov_action_id => voting_procedure } }; voter = [tag, hash]
        votes = M([([2, bytes(28)], M([([H32, 0], [1, None])]))])
        f = features(tx(body(k20=[proposal], k19=votes)))
        self.assertTrue({"gov-proposal:6", "vote:2"} <= f, f)

    def test_outputs(self):
        legacy = [BYRON_ADDR, 2_000_000]
        post = {0: SHELLEY_ADDR, 1: 2_000_000, 2: [1, T(24, b"\x01")], 3: T(24, b"\x82\x03\x41\x00")}
        f = features(tx(body(k1=[legacy, post])))
        self.assertTrue({"legacy-output", "byron-address", "inline-datum", "ref-script"} <= f, f)

    def test_tag258_and_indefinite(self):
        f = features(tx(body(k0=T(258, Indef([TXIN])))))
        self.assertTrue({"set-tag-258", "indefinite"} <= f, f)
        self.assertNotIn("set-no-tag", f)

    def test_witnesses(self):
        f = features(tx(body(), wits={3: [b"\x4e\x4d\x01"], 7: [b"\x4e\x4d\x01"]}))
        self.assertTrue({"plutus-v1", "plutus-v3"} <= f, f)

    def test_invalid_tx(self):
        self.assertIn("invalid-tx", features(tx(body(), valid=False)))

    def test_shelley_three_elements(self):
        f = features(enc([body(k5={b"\xe1" + bytes(28): 5}), {}, None]))
        self.assertIn("withdrawals", f)

    def test_garbage(self):
        for b in (b"", b"\x9f", b"\x80\x00", b"\x5f\x01\xff", b"\x7f\x41\x00\xff",
                  b"\x84\xa1\x00\x5f\x01\xff\xa0\xf5\xf6"):
            self.assertEqual(features(b), {"unparsed"}, b)

    def test_tags_vocabulary(self):
        seen = set()
        seen |= features(tx(body(k0=T(258, Indef([TXIN])), k5={b"\xe1" + bytes(28): 5}, k9={},
                                 k18=[], k22=1, k20=[[0, b"", [6], None]]),
                            wits={3: [], 7: []}, valid=False))
        seen |= features(b"")
        self.assertTrue(seen <= set(TAGS), seen - set(TAGS))


class BodyHash(unittest.TestCase):
    def test_hash_of_raw_body_bytes(self):
        b = body(k0=T(258, Indef([TXIN])))
        want = hashlib.blake2b(enc(b), digest_size=32).hexdigest()
        self.assertEqual(body_hash(tx(b)), want)

    def test_garbage_is_none(self):
        self.assertIsNone(body_hash(b"\x9f"))
        self.assertIsNone(body_hash(b"\x84\x5f\x01\xff"))


if __name__ == "__main__":
    unittest.main()
