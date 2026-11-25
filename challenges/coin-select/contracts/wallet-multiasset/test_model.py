import unittest

from cbor import Raw, encode
from model import (ADA, PARAMS, Output, min_ada, min_fee, min_lovelace, output_cbor,
                   output_size, tx_cbor, tx_size, utxo_ref, value_cbor, wallet_address)


class ModelTests(unittest.TestCase):
    def test_ada_only_judge_output(self):
        o = Output(wallet_address(0), (2_000_000, {}))
        self.assertEqual(output_size(o), 1 + 2 + 57 + 5)
        self.assertEqual(min_ada(o), (160 + 65) * 4310)

    def test_inline_datum_uses_map_form(self):
        o = Output(wallet_address(0), (5 * ADA, {}), ("inline", b"\x43abc"))
        self.assertEqual(encode(output_cbor(o))[0], 0xa3)

    def test_hash_datum_uses_array_form(self):
        o = Output(wallet_address(0), (5 * ADA, {}), ("hash", b"\x11" * 32))
        self.assertEqual(encode(output_cbor(o))[0], 0x83)

    def test_map_form_hash_datum_and_array_inline_error(self):
        h = Output(wallet_address(0), (5 * ADA, {}), ("hash", b"\x11" * 32))
        self.assertEqual(output_cbor(h, "map")[2], [0, b"\x11" * 32])
        i = Output(wallet_address(0), (5 * ADA, {}), ("inline", b"\x43abc"))
        with self.assertRaises(ValueError):
            output_cbor(i, "array")

    def test_canonical_asset_order(self):
        p1, p2 = b"\x02" * 28, b"\x01" * 28
        v = (2 * ADA, {p1: {b"bb": 1, b"a": 1}, p2: {b"z": 1}})
        m = value_cbor(v)[1]
        self.assertEqual(list(m), [p2, p1])
        self.assertEqual(list(m[p1]), [b"a", b"bb"])
        g = value_cbor(v, "given")[1]
        self.assertEqual(list(g), [p1, p2])
        self.assertEqual(list(g[p1]), [b"bb", b"a"])

    def test_length_first_name_order(self):
        p = b"\x01" * 28
        m = value_cbor((ADA, {p: {b"b": 1, b"aa": 1, b"": 1}}))[1][p]
        self.assertEqual(list(m), [b"", b"b", b"aa"])

    def test_input_index_width(self):
        one = tx_size([utxo_ref("d0#3", 3)], [], 0, 1)
        two = tx_size([utxo_ref("d0#30", 30)], [], 0, 1)
        self.assertEqual(two - one, 1)

    def test_tx_size_omits_is_valid_byte(self):
        ins, outs = [utxo_ref("d0#1", 1)], [Output(wallet_address(1), (2 * ADA, {}))]
        full = encode(tx_cbor(ins, [output_cbor(o) for o in outs], 200000, 1))
        self.assertEqual(tx_size(ins, outs, 200000, 1), len(full) - 1)

    def test_witness_cost(self):
        a = tx_size([utxo_ref("d0#1", 1)], [], 200000, 1)
        b = tx_size([utxo_ref("d0#1", 1)], [], 200000, 2)
        self.assertEqual(b - a, 101)

    def test_tag_sets_and_aux(self):
        ins = [utxo_ref("d0#1", 1)]
        plain = len(encode(tx_cbor(ins, [], 1, 1)))
        tagged = len(encode(tx_cbor(ins, [], 1, 1, tag_sets=True)))
        self.assertEqual(tagged - plain, 6)  # two tag-258 heads of 3 bytes
        a = len(encode(tx_cbor(ins, [], 1, 1, aux=Raw(b"\xa0"))))
        self.assertEqual(a, plain)  # null (1 byte) replaced by empty map (1 byte)
        e = tx_cbor(ins, [], 1, 1, extra_body=[(7, Raw(b"\x00"))])
        self.assertEqual(list(e[0]), [0, 1, 2, 7])

    def test_tag_witnesses_separately(self):
        ins = [utxo_ref("d0#1", 1)]
        only_ins = tx_cbor(ins, [], 1, 1, tag_sets=True, tag_witnesses=False)
        self.assertEqual(only_ins[0][0].tag, 258)
        self.assertIsInstance(only_ins[1][0], list)
        only_wits = tx_cbor(ins, [], 1, 1, tag_witnesses=True)
        self.assertIsInstance(only_wits[0][0], list)
        self.assertEqual(only_wits[1][0].tag, 258)

    def test_min_lovelace_fixed_point(self):
        assets = {b"\x01" * 28: {bytes([i]): 1 for i in range(40)}}
        l = min_lovelace(wallet_address(0), assets, None)
        self.assertGreaterEqual(l, min_ada(Output(wallet_address(0), (l, assets))))
        self.assertLess(l - 1, min_ada(Output(wallet_address(0), (l - 1, assets))))

    def test_min_fee(self):
        self.assertEqual(min_fee(300), 44 * 300 + 155381)

    def test_params_and_address(self):
        self.assertEqual(len(wallet_address(3)), 57)
        self.assertEqual(len(PARAMS["addresses"]), 8)
        self.assertEqual(PARAMS["addresses"][0], wallet_address(0).hex())
        self.assertEqual(utxo_ref("p1#2", 5)[1], 5)
        self.assertEqual(len(utxo_ref("p1#2", 5)[0]), 32)


if __name__ == "__main__":
    unittest.main()
