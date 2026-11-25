import json
import os
import subprocess
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
JUDGE = os.path.join(HERE, "judge")
CHALLENGE = os.path.dirname(os.path.dirname(HERE))
BASELINE = os.path.join(CHALLENGE, "submissions", "baseline-multiasset")
BROKEN = os.path.join(CHALLENGE, "preflight", "broken")

FAKE_CB = """#!/bin/sh
# fake cb: participant --report R -- CMD...
report="$3"
shift 4
"$@"
code=$?
printf '{"status":"ok","exit_code":0,"peak_rss_mib":1.0}' > "$report"
exit $code
"""


def run_judge(sub_dir, seed=3):
    with tempfile.TemporaryDirectory() as t:
        cb = os.path.join(t, "cb")
        with open(cb, "w") as f:
            f.write(FAKE_CB)
        os.chmod(cb, 0o755)
        env = dict(os.environ, CB_SEED=str(seed), CB_SUBMISSION_DIR=sub_dir, CB_BIN=cb)
        p = subprocess.run([JUDGE], env=env, capture_output=True, text=True, timeout=600)
    assert p.returncode == 0, p.stderr
    return json.loads(p.stdout)


def check(res, prop):
    return next(c for c in res["checks"] if c["property"] == prop)


def tiny_participant(body):
    d = tempfile.TemporaryDirectory()
    path = os.path.join(d.name, "select")
    with open(path, "w") as f:
        f.write("#!/usr/bin/env python3\nimport sys\n" + body)
    os.chmod(path, 0o755)
    return d


class JudgeTests(unittest.TestCase):
    def test_baseline_passes(self):
        if not os.path.isdir(BASELINE):
            self.skipTest("baseline not built yet")
        res = run_judge(BASELINE)
        self.assertTrue(res["ok"], res)
        self.assertTrue(all(c["ok"] for c in res["checks"]))
        self.assertEqual(sorted(res["axes"]), sorted(
            ["fees", "locked_ada", "ada_dust", "inputs_per_tx", "latency_ms", "peak_rss_mib"]))
        self.assertGreater(res["axes"]["fees"], 0)

    def test_linear_fee_broken(self):
        d = os.path.join(BROKEN, "ma-linear-fee")
        if not os.path.isdir(d):
            self.skipTest("broken variant not built yet")
        res = run_judge(d)
        self.assertFalse(res["ok"])
        c = check(res, "ma-valid-txs")
        self.assertFalse(c["ok"])
        self.assertIn("fee below min_fee", c["detail"])

    def test_broken_variants(self):
        names = sorted(n for n in os.listdir(BROKEN) if n.startswith("ma-"))
        if not names:
            self.skipTest("broken variants not built yet")
        for name in names:
            with self.subTest(name):
                d = os.path.join(BROKEN, name)
                with open(os.path.join(d, "EXPECT")) as f:
                    expect = f.read().strip()
                res = run_judge(d)
                self.assertFalse(res["ok"])
                self.assertFalse(res.get("infra", False), res)
                c = check(res, "ma-valid-txs")
                self.assertFalse(c["ok"])
                self.assertIn(expect, c["detail"])

    def test_refusing_participant(self):
        d = tiny_participant(
            "for line in sys.stdin:\n"
            "    if '\"type\":\"pay\"' in line:\n"
            "        print('{\"refuse\": true}', flush=True)\n")
        with d:
            res = run_judge(d.name)
        self.assertFalse(res["ok"])
        c = check(res, "ma-serves-feasible")
        self.assertFalse(c["ok"])
        self.assertIn("refused a payable payment", c["detail"])

    def test_unparseable_reply(self):
        d = tiny_participant(
            "for line in sys.stdin:\n"
            "    if '\"type\":\"pay\"' in line:\n"
            "        print('not json', flush=True)\n")
        with d:
            res = run_judge(d.name)
        self.assertFalse(res["ok"])
        c = check(res, "ma-valid-txs")
        self.assertFalse(c["ok"])
        self.assertIn("unparseable reply", c["detail"])


if __name__ == "__main__":
    unittest.main()
