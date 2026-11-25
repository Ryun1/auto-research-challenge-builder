"""Checks the byte model against real mainnet transactions, byte for byte.

fixtures/mainnet.jsonl holds Conway transactions fetched from Koios by
fixtures/fetch.py. Each one is decoded and rebuilt with model.tx_cbor and
model.output_cbor from its semantic content (inputs, outputs, fee, witness
count) in the forms it was found in. Body keys other than 0/1/2 and the
auxiliary data are carried over as raw bytes. The rebuilt bytes must equal the
original bytes, so the model's sizes, fees and min-ada are the ledger's.

Byte and size checks run on every fixture. Fee-floor and min-ada checks run
only where the tx's epoch params equal model.PARAMS (params_match).
"""
import json
import os
import unittest

from cbor import Raw, Tag, decode, decode_map_spans, encode
from model import Output, min_ada, min_fee, output_cbor, output_size, tx_cbor

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.join(HERE, "fixtures", "mainnet.jsonl")


def load(path=FIXTURES):
    """The fixture transactions (the meta header line is skipped)."""
    if not os.path.exists(path):
        return []
    with open(path) as f:
        rows = [json.loads(line) for line in f if line.strip()]
    return [r for r in rows if r.get("type") != "meta"]


def _array_spans(data, off):
    """[(start, end)] of each item of the definite-length array at data[off]."""
    ib = data[off]
    if ib >> 5 != 4:
        raise ValueError("not an array")
    info = ib & 31
    width = {24: 1, 25: 2, 26: 4, 27: 8}.get(info, 0)
    n = info if info < 24 else int.from_bytes(data[off + 1:off + 1 + width], "big")
    pos, spans = off + 1 + width, []
    for _ in range(n):
        _, end = decode(data, pos)
        spans.append((pos, end))
        pos = end
    return spans


def _untag(x):
    """(items, tagged) for a set that may carry tag 258."""
    if isinstance(x, Tag):
        if x.tag != 258:
            raise ValueError("unexpected tag %d" % x.tag)
        return x.value, True
    return x, False


def parse_output(raw: bytes):
    """(Output, form) from one output's original bytes."""
    o, _ = decode(raw, 0)
    if isinstance(o, list):
        form, addr, val = "array", o[0], o[1]
        datum = ("hash", o[2]) if len(o) == 3 else None
    else:
        form, addr, val = "map", o[0], o[1]
        datum = None
        if 2 in o:
            kind, body = o[2]
            datum = ("hash", body) if kind == 0 else ("inline", body.value)
    value = (val, {}) if isinstance(val, int) else (val[0], val[1])
    return Output(addr, value, datum), form


def form_of(raw: bytes) -> str:
    return "array" if raw[0] >> 5 == 4 else "map"


def split(f):
    """The original tx bytes split into its parts and spans."""
    data = bytes.fromhex(f["cbor"])
    parts = _array_spans(data, 0)
    if len(parts) != 4:
        raise ValueError("tx is not a 4-item array")
    return data, parts


def outputs(f):
    """[(original output bytes, Output)] in on-chain order."""
    data, parts = split(f)
    spans = {k: (s, e) for k, s, e in decode_map_spans(data, parts[0][0])}
    return [(data[s:e], parse_output(data[s:e])[0]) for s, e in _array_spans(data, spans[1][0])]


def rebuild(f):
    """The tx as model.tx_cbor would build it from the decoded content."""
    data, parts = split(f)
    body_spans = decode_map_spans(data, parts[0][0])
    body = {k: decode(data, s)[0] for k, s, _ in body_spans}
    ins, tag_inputs = _untag(body[0])
    outs = [output_cbor(o, form=form, order="given")
            for o, form in (parse_output(raw) for raw, _ in outputs(f))]
    wits_map, _ = decode(data, parts[1][0])
    wits, tag_witnesses = _untag(wits_map[0])
    extra = [(k, Raw(data[s:e])) for k, s, e in body_spans if k not in (0, 1, 2)]
    a_s, a_e = parts[3]
    return tx_cbor([tuple(i) for i in ins], outs, body[2], len(wits),
                   tag_sets=tag_inputs, tag_witnesses=tag_witnesses, extra_body=extra,
                   aux=Raw(data[a_s:a_e]))


def original_body(f):
    data, parts = split(f)
    s, e = parts[0]
    return data[s:e]


def rebuilt_body(f):
    return encode(rebuild(f)[0])


FIX = load()
MATCH = [f for f in FIX if f.get("params_match")]


@unittest.skipUnless(FIX, "fixtures/mainnet.jsonl missing; run fixtures/fetch.py")
class MainnetFixtureTests(unittest.TestCase):
    def test_enough_coverage(self):
        self.assertGreaterEqual(len(FIX), 40)
        self.assertGreaterEqual(sum(f["exact_min_fee"] for f in FIX), len(FIX) // 3)
        self.assertGreater(len(MATCH), len(FIX) * 3 // 4)

    def test_datum_kinds_covered(self):
        kinds = {out.datum[0] for f in FIX for _, out in outputs(f) if out.datum}
        self.assertEqual(kinds, {"inline", "hash"})

    def test_body_bytes_match(self):
        for f in FIX:
            self.assertEqual(rebuilt_body(f).hex(), original_body(f).hex(), f["hash"])

    def test_tx_size_matches(self):
        for f in FIX:
            self.assertEqual(len(encode(rebuild(f))), len(bytes.fromhex(f["cbor"])), f["hash"])

    def test_outputs_match_and_judge_form_never_longer(self):
        for f in FIX:
            for orig, out in outputs(f):
                got = encode(output_cbor(out, form=form_of(orig), order="given"))
                self.assertEqual(got.hex(), orig.hex(), f["hash"])
                self.assertLessEqual(output_size(out), len(orig), f["hash"])

    def test_fee_floor(self):
        for f in MATCH:
            mf = min_fee(len(bytes.fromhex(f["cbor"])) - 1)  # ledger size: no is_valid byte
            self.assertGreaterEqual(f["fee"], mf, f["hash"])
            self.assertEqual(f["exact_min_fee"], f["fee"] == mf, f["hash"])

    def test_min_ada(self):
        exact = 0
        for f in MATCH:
            for orig, out in outputs(f):
                need = (160 + len(orig)) * 4310
                self.assertGreaterEqual(out.value[0], need, f["hash"])
                exact += out.value[0] == need
        self.assertGreaterEqual(exact, 1)

    def test_judge_form_min_ada_never_above_ledger(self):
        for f in MATCH:
            for orig, out in outputs(f):
                self.assertLessEqual(min_ada(out), (160 + len(orig)) * 4310, f["hash"])

    def test_fixture_fee_is_body_fee(self):
        for f in FIX:
            self.assertEqual(rebuild(f)[0][2], f["fee"], f["hash"])


class FixtureFileTests(unittest.TestCase):
    def test_fixture_file_exists(self):
        self.assertTrue(FIX, "fixtures/mainnet.jsonl missing or empty")


if __name__ == "__main__":
    unittest.main()
