"""Tests for tricky.select (the curation choice; no network)."""
import unittest

from tricky import select


def cand(size, tags=(), pv=None):
    return {"bytes": size, "tags": set(tags), "pv": pv}


class Select(unittest.TestCase):
    def test_per_tag_takes_largest_first(self):
        cands = {f"h{n}": cand(n, ["mint"]) for n in range(1, 8)}
        self.assertEqual(sorted(select(cands, per_tag=5, largest=0)), ["h3", "h4", "h5", "h6", "h7"])

    def test_largest_overall(self):
        cands = {"a": cand(100), "b": cand(16_000), "c": cand(9_000), "d": cand(50)}
        self.assertEqual(sorted(select(cands, largest=2)), ["b", "c"])

    def test_pv_buckets(self):
        cands = {"a": cand(10, pv=8), "b": cand(20, pv=9), "c": cand(30, pv=10), "d": cand(5, pv=10)}
        self.assertEqual(sorted(select(cands, per_tag=1, largest=0)), ["a", "b", "c"])

    def test_cap(self):
        cands = {f"h{n:03}": cand(n, [f"cert:{n % 19}", "legacy-output"]) for n in range(500)}
        cands["rare"] = cand(1, ["gov-proposal:3"])
        got = select(cands, per_tag=20, cap=200)  # 19 cert buckets x 20 would be 380
        self.assertEqual(len(got), 200)
        self.assertIn("rare", got)
        tags = set().union(*(cands[h]["tags"] for h in got))
        self.assertEqual(tags, set().union(*(c["tags"] for c in cands.values())))

    def test_deterministic(self):
        cands = {f"h{n}": cand(n % 3, ["mint" if n % 2 else "withdrawals"]) for n in range(40)}
        self.assertEqual(select(cands, per_tag=3), select(dict(reversed(cands.items())), per_tag=3))

    def test_no_duplicates(self):
        cands = {"a": cand(10, ["mint", "withdrawals", "donation"], pv=11), "b": cand(5, ["mint"])}
        got = select(cands)
        self.assertEqual(len(got), len(set(got)))
        self.assertEqual(sorted(got), ["a", "b"])


if __name__ == "__main__":
    unittest.main()
