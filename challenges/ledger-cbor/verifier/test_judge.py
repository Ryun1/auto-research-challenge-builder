"""Tests for judge.build_cases, judge.tricky and judge.verify, with a fake harness."""
import gzip, hashlib, json, random, tempfile, unittest
from pathlib import Path

import judge

MUTATIONS = {"truncate", "flip", "append", "prefix"}


def synthetic(n, seed=1):
    rng = random.Random(seed)
    return [rng.randbytes(rng.randrange(5, 50)) for _ in range(n)]


POOL, HARD = synthetic(900), synthetic(5, seed=2)


class Cases(unittest.TestCase):
    def cases(self, seed=7, p=POOL, hard=HARD):
        return judge.build_cases(seed, p, hard)

    def test_valid_is_whole_pool(self):
        self.assertEqual([t for k, t in self.cases() if k == "valid"], POOL)

    def test_scored_txs_are_verified(self):
        valid = {t for k, t in self.cases() if k == "valid"}
        self.assertLessEqual(set(judge.score_sample(7, POOL)), valid)

    def test_pool_mutants(self):
        self.assertEqual(sum(k in MUTATIONS for k, _ in self.cases()), 600)
        self.assertEqual(sum(k in MUTATIONS for k, _ in self.cases(p=POOL[:10])), 10)

    def test_each_hard_case_and_mutant(self):
        cases = self.cases()
        self.assertEqual([t for k, t in cases if k == "tricky"], HARD)
        muts = [k for k, _ in cases if k.startswith("tricky-")]
        self.assertEqual(len(muts), len(HARD))
        self.assertTrue(all(k.removeprefix("tricky-") in MUTATIONS for k in muts), muts)

    def test_edge_cases_last(self):
        self.assertEqual([k for k, _ in self.cases()[-4:]], ["edge"] * 4)

    def test_seeded(self):
        self.assertEqual(self.cases(seed=7), self.cases(seed=7))
        self.assertNotEqual(self.cases(seed=7), self.cases(seed=8))


class FakeHarness:
    """Stands in for judge.lines: answers per input from dicts keyed by input bytes."""

    def __init__(self, ref=None, cand=None):
        self.ref, self.cand = ref or {}, cand or {}

    def __call__(self, exe, mode, path):
        inputs = [bytes.fromhex(l) for l in Path(path).read_text().split("\n")[:-1]]
        out = []
        for b in inputs:
            r = self.ref.get(b, "ok " + hashlib.sha256(b).hexdigest()[:16])
            line = r if exe == judge.REF else self.cand.get(b, r)
            out.append(line if mode == "digest" else "nf 0 0")
        return out


class Verify(unittest.TestCase):
    def setUp(self):
        self.saved = judge.pool, judge.tricky, judge.lines
        judge.pool, judge.tricky = (lambda: POOL), (lambda: HARD)

    def tearDown(self):
        judge.pool, judge.tricky, judge.lines = self.saved

    def run_verify(self, **fake):
        judge.lines = FakeHarness(**fake)
        return judge.verify(7)

    def test_all_agree(self):
        r = self.run_verify()
        self.assertTrue(r["ok"], r)
        self.assertIn("includes the 800 scored txs", r["detail"])

    def test_reference_rejects_hard_case_not_infra(self):
        r = self.run_verify(ref={HARD[0]: "err"})
        self.assertTrue(r["ok"], r)
        self.assertNotIn("infra", r)
        self.assertIn("tricky: 4 accepted, 1 rejected by reference", r["detail"])

    def test_candidate_differs_on_hard_case(self):
        r = self.run_verify(cand={HARD[3]: "err"})
        self.assertFalse(r["ok"])
        self.assertNotIn("infra", r)
        self.assertIn(" tricky ", r["detail"])

    def test_candidate_differs_on_hard_mutant(self):
        mutant = next(t for k, t in judge.build_cases(7, POOL, HARD) if k.startswith("tricky-"))
        r = self.run_verify(cand={mutant: "ok accepts-a-mutant"})
        self.assertFalse(r["ok"])
        self.assertIn("tricky-", r["detail"])

    def test_reference_rejects_pool_tx_is_infra(self):
        r = self.run_verify(ref={POOL[5]: "err"})
        self.assertTrue(r.get("infra"), r)

    def test_unloadable_hard_cases_is_infra(self):
        def broken():
            raise ValueError("tricky.hex.gz has 2 txs, tricky.json 3")
        judge.tricky = broken
        r = self.run_verify()
        self.assertFalse(r["ok"])
        self.assertTrue(r.get("infra"), r)
        self.assertIn("hard-case corpus", r["detail"])


class LoadTricky(unittest.TestCase):
    def test_tricky_out_of_sync(self):
        with tempfile.TemporaryDirectory() as d:
            hexgz = Path(d) / "tricky.hex.gz"
            with gzip.open(hexgz, "wt") as f:
                f.write("00\n01\n")
            (Path(d) / "tricky.json").write_text(json.dumps({"txs": [{}, {}, {}]}))
            with self.assertRaises(ValueError):
                judge.tricky(hexgz)

    def test_committed_tricky_loads(self):
        hard = judge.tricky()
        self.assertTrue(1 <= len(hard) <= 200, len(hard))
        self.assertTrue(all(isinstance(t, bytes) and t for t in hard))


if __name__ == "__main__":
    unittest.main()
