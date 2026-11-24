import unittest

from cbor import encode, decode, Tag, Raw, NonCanonical, decode_map_spans


class TestCbor(unittest.TestCase):
    def test_uint_widths(self):
        for n, ln in [(0, 1), (23, 1), (24, 2), (255, 2), (256, 3), (65535, 3),
                      (65536, 5), (2**32 - 1, 5), (2**32, 9)]:
            self.assertEqual(len(encode(n)), ln)

    def test_roundtrip(self):
        v = [1, b"\x00" * 28, {0: b"ab", 1: [2, {b"p": {b"": 5}}]}, Tag(24, b"\x01"), True, None]
        self.assertEqual(decode(encode(v))[0], v)

    def test_raw_passthrough(self):
        self.assertEqual(encode([Raw(b"\x01")]), b"\x81\x01")

    def test_rejects_indefinite_and_nonminimal(self):
        for bad in (b"\x9f\x01\xff", b"\x18\x05"):
            with self.assertRaises(NonCanonical):
                decode(bad)

    def test_map_spans(self):
        d = encode({0: 1, 3: 99999})
        spans = decode_map_spans(d, 0)
        self.assertEqual([k for k, _, _ in spans], [0, 3])
        k, s, e = spans[1]
        self.assertEqual(decode(d[s:e])[0], 99999)

    def test_negative_str_and_duplicates(self):
        self.assertEqual(encode(-1), b"\x20")
        self.assertEqual(encode(-25), b"\x38\x18")
        self.assertEqual(decode(encode(-1000))[0], -1000)
        self.assertEqual(decode(encode("hi"))[0], "hi")
        with self.assertRaises(NonCanonical):
            decode(b"\xa2\x01\x01\x01\x02")
        with self.assertRaises(ValueError):
            encode(1.5)
        with self.assertRaises(ValueError):
            decode(b"\xfb" + b"\x00" * 8)


if __name__ == "__main__":
    unittest.main()
