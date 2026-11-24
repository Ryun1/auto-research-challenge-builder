import unittest

from model import (ADA, PARAMS, Output, min_ada, min_fee, min_lovelace, tx_size, utxo_ref,
                   value_size, wallet_address)
from sim import (N_EVENTS, HashRandom, Utxo, add_deposit, ada_dust, apply_tx, gen_scenario,
                 gen_scenarios, locked_ada, parse_output, parse_value, reference,
                 simulate_reference, validate_tx, value_json)

POL_A = b"\xaa" * 28
POL_B = b"\xbb" * 28
TOK = b"token-a"
DEST = b"\x01" + b"\x22" * 56


def utxo(uid, addr, lovelace, assets=None, datum=None):
    idx = int(uid.split("#")[1])
    return Utxo(utxo_ref(uid, idx), addr, (lovelace, assets or {}), datum)


W = {
    "d0#0": utxo("d0#0", 0, 20 * ADA, {POL_A: {TOK: 1000}, POL_B: {b"": 5}}),
    "d0#1": utxo("d0#1", 1, 10 * ADA, None, ("inline", b"\x43abc")),
    "d0#2": utxo("d0#2", 0, 4 * ADA),
}
OUTS = [Output(DEST, (3 * ADA, {POL_A: {TOK: 400}}))]


def change_outputs(reply):
    return [Output(wallet_address(c["addr"]), parse_value(c["value"])) for c in reply["change"]]


def exact_fee(wallet, outs, reply):
    """Set reply's fee to min_fee and move the difference into change 0's lovelace."""
    spare = (sum(wallet[i].value[0] for i in reply["inputs"])
             - sum(o.value[0] for o in outs)
             - sum(c["value"]["lovelace"] for c in reply["change"]) - reply["fee"])
    reply["change"][0]["value"]["lovelace"] += spare
    refs = [wallet[i].ref for i in reply["inputs"]]
    nw = len({wallet[i].addr for i in reply["inputs"]})
    while True:
        f = min_fee(tx_size(refs, outs + change_outputs(reply), reply["fee"], nw))
        if f == reply["fee"]:
            return reply
        reply["change"][0]["value"]["lovelace"] -= f - reply["fee"]
        reply["fee"] = f


def good_reply():
    """Spend d0#0: change 0 carries the leftover tokens, change 1 the ADA."""
    tok = {"lovelace": 0, "assets": {POL_A.hex(): {TOK.hex(): 600}, POL_B.hex(): {"": 5}}}
    tok["lovelace"] = min_lovelace(wallet_address(0), parse_value(tok)[1], None)
    r = {"inputs": ["d0#0"],
         "change": [{"addr": 2, "value": {"lovelace": 0}}, {"addr": 0, "value": tok}],
         "fee": 0}
    return exact_fee(W, OUTS, r)


class ParseTests(unittest.TestCase):
    def test_parse_value_accepts(self):
        self.assertEqual(parse_value({"lovelace": 5}), (5, {}))
        self.assertEqual(parse_value({"lovelace": 5, "assets": {}}), (5, {}))
        j = {"lovelace": 5, "assets": {"ab" * 28: {"": 1, "cd" * 32: 2 ** 64 - 1}}}
        v = parse_value(j)
        self.assertEqual(v, (5, {b"\xab" * 28: {b"": 1, b"\xcd" * 32: 2 ** 64 - 1}}))
        self.assertEqual(parse_value(value_json(v)), v)

    def test_parse_value_rejects(self):
        pol = "ab" * 28
        bad = [
            None, [], {"assets": {}}, {"lovelace": -1}, {"lovelace": True}, {"lovelace": 1.0},
            {"lovelace": 2 ** 64}, {"lovelace": 1, "extra": 1}, {"lovelace": 1, "assets": []},
            {"lovelace": 1, "assets": {pol: {}}},
            {"lovelace": 1, "assets": {pol: []}},
            {"lovelace": 1, "assets": {pol: {"cd" * 33: 1}}},
            {"lovelace": 1, "assets": {pol: {"abc": 1}}},
            {"lovelace": 1, "assets": {pol: {"zz": 1}}},
            {"lovelace": 1, "assets": {pol: {"AB": 1}}},
            {"lovelace": 1, "assets": {pol: {"a b ": 1}}},
            {"lovelace": 1, "assets": {"AB" * 28: {"": 1}}},
            {"lovelace": 1, "assets": {pol: {"": 0}}},
            {"lovelace": 1, "assets": {pol: {"": -3}}},
            {"lovelace": 1, "assets": {pol: {"": True}}},
            {"lovelace": 1, "assets": {pol: {"": 1.0}}},
            {"lovelace": 1, "assets": {pol: {"": "1"}}},
            {"lovelace": 1, "assets": {pol: {"": 2 ** 64}}},
        ]
        for j in bad:
            with self.assertRaises(ValueError, msg=repr(j)):
                parse_value(j)

    def test_parse_output(self):
        v = {"lovelace": 2 * ADA}
        for n in (57, 29):
            o = parse_output({"address": "01" * n, "value": v})
            self.assertEqual(o, Output(b"\x01" * n, (2 * ADA, {}), None))
        o = parse_output({"address": "11" * 57, "value": v, "datum": {"inline": "43616263"}})
        self.assertEqual(o.datum, ("inline", b"\x43abc"))
        o = parse_output({"address": "11" * 57, "value": v, "datum": {"hash": "ee" * 32}})
        self.assertEqual(o.datum, ("hash", b"\xee" * 32))
        bad = [
            {"address": "01" * 28, "value": v}, {"address": "01" * 58, "value": v},
            {"address": "0", "value": v}, {"value": v}, {"address": "01" * 57},
            {"address": "01" * 57, "value": v, "datum": {"hash": "ee" * 31}},
            {"address": "01" * 57, "value": v, "datum": {"inline": ""}},
            {"address": "01" * 57, "value": v, "datum": {"inline": "43", "hash": "ee" * 32}},
            {"address": "01" * 57, "value": v, "datum": {"other": "43"}},
            {"address": "01" * 57, "value": v, "datum": None},
            {"address": "01" * 57, "value": v, "extra": 1},
        ]
        for j in bad:
            with self.assertRaises(ValueError, msg=repr(j)):
                parse_output(j)


class ValidatorTests(unittest.TestCase):
    def test_good_reply_valid(self):
        self.assertIsNone(validate_tx(W, OUTS, good_reply()))

    def test_structural(self):
        g = good_reply()
        bad = {
            "not object": [1],
            "inputs not list": dict(g, inputs="d0#0"),
            "change not list": dict(g, change={}),
            "bool fee": dict(g, fee=True),
            "float fee": dict(g, fee=1.0),
            "negative fee": dict(g, fee=-1),
            "no inputs": dict(g, inputs=[]),
            "int input": dict(g, inputs=[0]),
            "duplicate": dict(g, inputs=["d0#0", "d0#0"]),
            "unknown": dict(g, inputs=["zz#0"]),
            "change not object": dict(g, change=[5]),
            "change missing value": dict(g, change=[{"addr": 0}]),
            "change datum": dict(g, change=[dict(g["change"][0], datum={"hash": "ee" * 32}),
                                            g["change"][1]]),
        }
        for name, r in bad.items():
            self.assertIsNotNone(validate_tx(W, OUTS, r), name)

    def test_rejects_bad_asset_keys(self):
        for bad in ["ab" * 27, "ab" * 29, "abc", "zz" * 28]:
            r = good_reply()
            r["change"][1]["value"]["assets"] = {bad: {"": 1}}
            self.assertEqual(validate_tx(W, OUTS, r), "bad value")

    def test_zero_qty_rejected_split_allowed(self):
        r = good_reply()
        r["change"][1]["value"]["assets"][POL_A.hex()][TOK.hex()] = 0
        self.assertEqual(validate_tx(W, OUTS, r), "bad value")
        # split POL_A's 600 across both change outputs: 1 + 599
        r = good_reply()
        r["change"][1]["value"]["assets"][POL_A.hex()][TOK.hex()] = 599
        r["change"][0]["value"]["assets"] = {POL_A.hex(): {TOK.hex(): 1}}
        r["change"][0]["value"]["lovelace"] -= 200_000
        r["fee"] += 200_000  # covers the growth of change 0
        self.assertIsNone(validate_tx(W, OUTS, r))

    def test_asset_imbalance(self):
        r = good_reply()
        r["change"][1]["value"]["assets"][POL_A.hex()][TOK.hex()] = 599
        self.assertEqual(validate_tx(W, OUTS, r), "asset imbalance")
        r = good_reply()
        r["change"][1]["value"]["assets"][POL_A.hex()][TOK.hex()] = 601
        self.assertEqual(validate_tx(W, OUTS, r), "asset imbalance")
        r = good_reply()  # dropping a whole policy
        del r["change"][1]["value"]["assets"][POL_B.hex()]
        self.assertEqual(validate_tx(W, OUTS, r), "asset imbalance")
        r = good_reply()  # minting an asset the inputs don't hold
        r["change"][1]["value"]["assets"]["cc" * 28] = {"": 1}
        self.assertEqual(validate_tx(W, OUTS, r), "asset imbalance")

    def test_lovelace_imbalance(self):
        r = good_reply()
        r["change"][0]["value"]["lovelace"] += 1
        self.assertEqual(validate_tx(W, OUTS, r), "imbalance")

    def test_change_below_own_min_ada(self):
        r = good_reply()
        r["change"][1]["value"]["lovelace"] -= 1
        r["change"][0]["value"]["lovelace"] += 1
        self.assertEqual(validate_tx(W, OUTS, r), "change below min_ada")

    def test_linear_fee_rejected(self):
        r = good_reply()
        linear = 44 * (60 + 140 * 1 + 65 * 3) + 155381  # wallet@1's model
        self.assertLess(linear, r["fee"])
        r["change"][0]["value"]["lovelace"] += r["fee"] - linear
        r["fee"] = linear
        self.assertEqual(validate_tx(W, OUTS, r), "fee below min_fee")

    def test_fee_one_low(self):
        r = good_reply()
        r["change"][0]["value"]["lovelace"] += 1
        r["fee"] -= 1
        self.assertEqual(validate_tx(W, OUTS, r), "fee below min_fee")

    def test_bad_addr(self):
        for addr in (8, -1, True, "0", None, 1.0):
            r = good_reply()
            r["change"][0]["addr"] = addr
            self.assertEqual(validate_tx(W, OUTS, r), "bad change addr", repr(addr))

    def test_witness_count_from_keys(self):
        one = {"d1#0": utxo("d1#0", 3, 6 * ADA), "d1#1": utxo("d1#1", 3, 6 * ADA)}
        two = {"d1#0": utxo("d1#0", 3, 6 * ADA), "d1#1": utxo("d1#1", 5, 6 * ADA)}
        outs = [Output(DEST, (5 * ADA, {}))]
        r = exact_fee(one, outs, {"inputs": ["d1#0", "d1#1"],
                                  "change": [{"addr": 0, "value": {"lovelace": 0}}], "fee": 0})
        self.assertIsNone(validate_tx(one, outs, r))
        self.assertEqual(validate_tx(two, outs, r), "fee below min_fee")
        r2 = exact_fee(two, outs, {"inputs": ["d1#0", "d1#1"],
                                   "change": [{"addr": 0, "value": {"lovelace": 0}}], "fee": 0})
        self.assertEqual(r2["fee"] - r["fee"], 101 * 44)
        self.assertIsNone(validate_tx(two, outs, r2))

    def test_change_over_max_val_size(self):
        w, assets = big_bundle_wallet()
        outs = [Output(DEST, (2 * ADA, {}))]
        r = exact_fee(w, outs, {"inputs": ["d0#0"], "change": [
            {"addr": 0, "value": {"lovelace": 0, "assets": value_json((0, assets))["assets"]}}],
            "fee": 0})
        self.assertGreater(value_size(parse_value(r["change"][0]["value"])), 5000)
        self.assertEqual(validate_tx(w, outs, r), "change over max_val_size")

    def test_size_over_max(self):
        w = {"d0#%d" % i: utxo("d0#%d" % i, i % 8, 2 * ADA) for i in range(500)}
        outs = [Output(DEST, (100 * ADA, {}))]
        r = {"inputs": list(w), "change": [], "fee": 900 * ADA}
        err = validate_tx(w, outs, r)
        self.assertTrue(err and "over max" in err, err)

    def test_size_over_max_past_cheap_precheck(self):
        # 440 inputs pass the 36/input pre-check (15840 + 60 < 16384) but the real
        # tx (inputs, 101-byte witnesses, outputs) is over max_tx_size.
        w = {"d0#%d" % i: utxo("d0#%d" % i, i % 8, 2 * ADA) for i in range(440)}
        outs = [Output(DEST, (100 * ADA, {}))]
        r = {"inputs": list(w), "change": [], "fee": 780 * ADA}
        err = validate_tx(w, outs, r)
        self.assertTrue(err and err.startswith("size ") and err.endswith(" over max"), err)


def big_bundle_wallet():
    """One UTxO with 3 policies x 50 names of 32 bytes (about 5.4 KB of value)."""
    assets = {bytes([p]) * 28: {bytes([p, n]) * 16: 1 for n in range(50)} for p in (1, 2, 3)}
    return {"d0#0": utxo("d0#0", 4, 40 * ADA, assets)}, assets


class ReferenceTests(unittest.TestCase):
    def test_reference_on_small_wallet(self):
        r = reference(W, OUTS)
        self.assertIsNotNone(r)
        self.assertIsNone(validate_tx(W, OUTS, r))
        self.assertEqual(r["inputs"], ["d0#0"])  # the only holder of POL_A
        self.assertTrue(all(c["addr"] == 0 for c in r["change"]))
        self.assertEqual(len(r["change"]), 1)
        self.assertEqual(parse_value(r["change"][0]["value"])[1],
                         {POL_A: {TOK: 600}, POL_B: {b"": 5}})

    def test_reference_largest_first_lovelace(self):
        outs = [Output(DEST, (12 * ADA, {}))]
        r = reference(W, outs)
        self.assertEqual(r["inputs"], ["d0#0"])
        outs = [Output(DEST, (25 * ADA, {}))]
        r = reference(W, outs)
        self.assertEqual(r["inputs"], ["d0#0", "d0#1"])
        self.assertIsNone(validate_tx(W, outs, r))

    def test_reference_none(self):
        self.assertIsNone(reference(W, [Output(DEST, (40 * ADA, {}))]))
        self.assertIsNone(reference(W, [Output(DEST, (2 * ADA, {POL_A: {TOK: 1001}}))]))
        self.assertIsNone(reference(W, [Output(DEST, (2 * ADA, {b"\xcc" * 28: {b"": 1}}))]))

    def test_reference_fold(self):
        outs = [Output(DEST, (3 * ADA, {}))]
        w = {"d0#0": utxo("d0#0", 0, 3 * ADA + 600_000)}
        r = reference(w, outs)
        self.assertEqual(r["change"], [])
        self.assertEqual(r["fee"], 600_000)
        self.assertIsNone(validate_tx(w, outs, r))

    def test_reference_splits_oversized_change(self):
        w, assets = big_bundle_wallet()
        outs = [Output(DEST, (2 * ADA, {}))]
        r = reference(w, outs)
        self.assertIsNotNone(r)
        self.assertGreater(len(r["change"]), 1)
        self.assertIsNone(validate_tx(w, outs, r))
        for c in r["change"]:
            self.assertLessEqual(value_size(parse_value(c["value"])), PARAMS["max_val_size"])

    def test_reference_splits_one_oversized_policy(self):
        assets = {b"\x07" * 28: {bytes([n]) * 32: 1 for n in range(160)}}
        w = {"d0#0": utxo("d0#0", 0, 40 * ADA, assets)}
        outs = [Output(DEST, (2 * ADA, {}))]
        r = reference(w, outs)
        self.assertGreater(len(r["change"]), 1)
        self.assertIsNone(validate_tx(w, outs, r))

    def test_reference_valid_on_all_payments(self):
        # simulate_reference raises unless every reference reply validates
        for sc in gen_scenarios(11)[:2]:
            txs, fees = simulate_reference(sc)
            self.assertGreater(txs, 150)
            self.assertGreater(fees, 0)


class WalletTests(unittest.TestCase):
    def test_add_deposit_and_apply_tx(self):
        w = {}
        add_deposit(w, {"t": "deposit", "utxos": [
            {"id": "d0#0", "index": 0, "addr": 2, "value": {"lovelace": 9 * ADA}},
            {"id": "d0#1", "index": 1, "addr": 3, "value": {"lovelace": 2 * ADA},
             "datum": {"hash": "ee" * 32}}]})
        self.assertEqual(w["d0#0"], Utxo(utxo_ref("d0#0", 0), 2, (9 * ADA, {}), None))
        self.assertEqual(w["d0#1"].datum, ("hash", b"\xee" * 32))
        reply = {"inputs": ["d0#0"], "change": [{"addr": 5, "value": {"lovelace": 3 * ADA}},
                                                {"addr": 6, "value": {"lovelace": 2 * ADA}}],
                 "fee": 200_000}
        apply_tx(w, "p4", 2, reply)
        self.assertNotIn("d0#0", w)
        self.assertEqual(w["p4#0"], Utxo(utxo_ref("p4#0", 2), 5, (3 * ADA, {}), None))
        self.assertEqual(w["p4#1"].ref, utxo_ref("p4", 3))

    def test_locked_ada_and_dust(self):
        expect = sum(min_ada(Output(wallet_address(u.addr), u.value, u.datum)) for u in W.values())
        self.assertEqual(locked_ada(W), expect)
        self.assertEqual(ada_dust(W), 0)
        w = dict(W, x=utxo("x#0", 0, 2 * ADA - 1), y=utxo("y#0", 0, ADA, {POL_A: {TOK: 1}}))
        self.assertEqual(ada_dust(w), 1)


class GeneratorTests(unittest.TestCase):
    def test_deterministic(self):
        self.assertEqual(gen_scenarios(7), gen_scenarios(7))
        self.assertNotEqual(gen_scenarios(7), gen_scenarios(8))
        self.assertEqual(gen_scenario(HashRandom(7, 3)), gen_scenarios(7)[3])

    def test_events_well_formed(self):
        for sc in gen_scenarios(2):
            self.assertEqual(len(sc), N_EVENTS + 1)
            self.assertEqual(sc[0]["t"], "deposit")
            self.assertTrue(60 <= len(sc[0]["utxos"]) <= 120)
            w = {}
            for e in sc:
                if e["t"] == "deposit":
                    for k, u in enumerate(e["utxos"]):
                        self.assertEqual(u["index"], k)
                        self.assertTrue(0 <= u["addr"] < 8)
                    add_deposit(w, e)
                    for u in e["utxos"]:
                        x = w[u["id"]]
                        self.assertGreaterEqual(
                            x.value[0], min_lovelace(wallet_address(x.addr), x.value[1], x.datum))
                else:
                    self.assertTrue(1 <= len(e["outputs"]) <= 3)
                    for j in e["outputs"]:
                        o = parse_output(j)
                        self.assertGreaterEqual(
                            o.value[0], min_lovelace(o.address, o.value[1], o.datum))
                        self.assertLessEqual(value_size(o.value), PARAMS["max_val_size"])
                        if o.datum:
                            self.assertEqual(o.address[0], 0x11)

    def test_reference_serves_all(self):
        for seed in range(5):
            for sc in gen_scenarios(seed):
                txs, _ = simulate_reference(sc)
                self.assertGreater(txs, 150)

    def test_scenario_mix(self):
        pays = [e for sc in gen_scenarios(1) for e in sc if e["t"] == "pay"]
        tok = sum(any(o["value"].get("assets") for o in e["outputs"]) for e in pays) / len(pays)
        dat = sum(any("datum" in o for o in e["outputs"]) for e in pays) / len(pays)
        self.assertTrue(0.12 < tok < 0.3 and 0.05 < dat < 0.2, (tok, dat))
        deps = [u for sc in gen_scenarios(1) for e in sc if e["t"] == "deposit"
                for u in e["utxos"]]
        self.assertTrue(any(len(u["value"].get("assets", {})) for u in deps))
        self.assertTrue(any("inline" in u.get("datum", {}) for u in deps))
        self.assertTrue(any("hash" in u.get("datum", {}) for u in deps))


if __name__ == "__main__":
    unittest.main()
