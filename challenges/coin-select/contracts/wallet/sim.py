#!/usr/bin/env python3
"""Scenario generator, size/fee/min-ada model, transaction validator and the
largest-first reference for contract wallet@1. Trusted judge code, stdlib only.

`python3 sim.py --self-test` runs the built-in checks.
"""
import math
import random
import sys

ADA = 1_000_000
PARAMS = {
    "type": "params",
    "min_fee_a": 44,
    "min_fee_b": 155381,
    "coins_per_utxo_byte": 4310,
    "max_tx_size": 16384,
    "size": {"base": 60, "input": 140, "output": 65},
}
N_SCENARIOS = 8
N_EVENTS = 300
FEE_BUDGET = 1 * ADA  # fundability assumes every earlier tx cost at most this


def tx_size(n_in, n_out, p=PARAMS):
    s = p["size"]
    return s["base"] + s["input"] * n_in + s["output"] * n_out


def min_fee(n_in, n_out, p=PARAMS):
    return p["min_fee_a"] * tx_size(n_in, n_out, p) + p["min_fee_b"]


def min_ada(p=PARAMS):
    return (160 + p["size"]["output"]) * p["coins_per_utxo_byte"]


# ---------------------------------------------------------------- generator

def _lognorm(rng, median_ada, sigma):
    return int(rng.lognormvariate(math.log(median_ada * ADA), sigma))


def _deposit_utxo(rng):
    if rng.random() < 0.7:
        return max(ADA, _lognorm(rng, 30, 1.2))
    return rng.randint(1 * ADA, 3 * ADA)  # reward-like


def gen_scenario(rng):
    """One scenario: a list of events. Event 0 is the initial deposit."""
    ev = []

    def deposit(values):
        i = len(ev)
        ev.append({"t": "deposit", "utxos": [
            {"id": "d%d#%d" % (i, k), "lovelace": v} for k, v in enumerate(values)]})
        return sum(values)

    n0 = rng.randint(60, 120)
    balance = deposit([max(int(1.5 * ADA), _lognorm(rng, 50, 1.0)) for _ in range(n0)])
    while len(ev) < N_EVENTS + 1:
        if rng.random() < 0.3:
            balance += deposit([_deposit_utxo(rng) for _ in range(rng.randint(1, 5))])
            continue
        outs = [max(ADA, _lognorm(rng, 20, 1.0)) for _ in range(rng.randint(1, 3))]
        total = sum(outs)
        while balance - (total + FEE_BUDGET) < 3 * (total + FEE_BUDGET):
            balance += deposit([_deposit_utxo(rng) for _ in range(rng.randint(1, 5))])
        i = len(ev)
        ev.append({"t": "pay", "id": "p%d" % i, "outputs": outs})
        balance -= total + FEE_BUDGET
    return ev[:N_EVENTS + 1]  # any prefix is fundable


def gen_scenarios(seed):
    return [gen_scenario(random.Random(seed * 1000 + k)) for k in range(N_SCENARIOS)]


# ---------------------------------------------------------------- validator

def _is_int(x):
    return isinstance(x, int) and not isinstance(x, bool)


def validate_tx(wallet, outputs, reply, p=PARAMS):
    """Return None when `reply` is a valid spend of `wallet` for `outputs`
    (a list of lovelace amounts), else a short reason."""
    if not isinstance(reply, dict):
        return "reply is not an object"
    ins, change, fee = reply.get("inputs"), reply.get("change"), reply.get("fee")
    if not isinstance(ins, list) or not isinstance(change, list):
        return "inputs and change must be lists"
    if not _is_int(fee) or fee < 0:
        return "fee is not a non-negative integer"
    if not ins:
        return "no inputs"
    if not all(isinstance(i, str) for i in ins):
        return "input ids must be strings"
    if len(set(ins)) != len(ins):
        return "duplicate input"
    if any(i not in wallet for i in ins):
        return "unknown input"
    if not all(_is_int(c) and c >= 0 for c in change):
        return "change is not non-negative integers"
    if sum(wallet[i] for i in ins) != sum(outputs) + sum(change) + fee:
        return "imbalance"
    size = tx_size(len(ins), len(outputs) + len(change), p)
    if size > p["max_tx_size"]:
        return "size %d over max" % size
    if fee < min_fee(len(ins), len(outputs) + len(change), p):
        return "fee below min_fee"
    if any(c < min_ada(p) for c in change):
        return "change below min_ada"
    return None


# --------------------------------------------------------- largest-first

def largest_first(wallet, outputs, p=PARAMS):
    """The judge's reference. Returns a reply dict or None if the whole
    wallet cannot pay (within max_tx_size)."""
    need = sum(outputs)
    order = sorted(wallet, key=lambda i: (-wallet[i], i))
    total, picked = 0, []
    for uid in order:
        picked.append(uid)
        total += wallet[uid]
        n = len(picked)
        if tx_size(n, len(outputs) + 1, p) > p["max_tx_size"]:
            return None
        spare = total - need
        fee_c = min_fee(n, len(outputs) + 1, p)
        if spare - fee_c >= min_ada(p):
            return {"inputs": list(picked), "change": [spare - fee_c], "fee": fee_c}
        if spare >= min_fee(n, len(outputs), p):
            return {"inputs": list(picked), "change": [], "fee": spare}
    return None


def apply_tx(wallet, pay_id, reply):
    for i in reply["inputs"]:
        del wallet[i]
    for k, c in enumerate(reply["change"]):
        wallet["%s#%d" % (pay_id, k)] = c


def simulate_largest_first(scenario):
    """Run a scenario in-process; returns (txs, fees) or raises."""
    wallet = {}
    txs = fees = 0
    for e in scenario:
        if e["t"] == "deposit":
            for u in e["utxos"]:
                wallet[u["id"]] = u["lovelace"]
        else:
            r = largest_first(wallet, e["outputs"])
            if r is None:
                raise AssertionError("unfundable " + e["id"])
            err = validate_tx(wallet, e["outputs"], r)
            if err:
                raise AssertionError("reference invalid: " + err)
            apply_tx(wallet, e["id"], r)
            txs += 1
            fees += r["fee"]
    return txs, fees


# --------------------------------------------------------------- self-test

def self_test():
    ma = min_ada()
    assert ma == 225 * 4310
    w = {"a": 10 * ADA, "b": 5 * ADA, "c": 1_000_000}
    outs = [3 * ADA]
    fee = min_fee(1, 2)
    assert fee == 44 * (60 + 140 + 65 * 2) + 155381
    good = {"inputs": ["a"], "change": [10 * ADA - 3 * ADA - fee], "fee": fee}
    assert validate_tx(w, outs, good) is None
    bad = {
        "fee one low": {"inputs": ["a"], "change": [10 * ADA - 3 * ADA - fee + 1], "fee": fee - 1},
        "imbalance": {"inputs": ["a"], "change": [10 * ADA - 3 * ADA - fee - 1], "fee": fee},
        "duplicate": {"inputs": ["a", "a"], "change": [17 * ADA - fee], "fee": fee},
        "unknown": {"inputs": ["zz"], "change": [], "fee": 0},
        "empty": {"inputs": [], "change": [], "fee": 0},
        "dust change": {"inputs": ["a"], "change": [ma - 1, 7 * ADA - fee - ma + 1], "fee": fee},
        "negative": {"inputs": ["a"], "change": [-1, 7 * ADA - fee + 1], "fee": fee},
        "float": {"inputs": ["a"], "change": [7 * ADA - fee + 0.5], "fee": fee},
        "bool fee": {"inputs": ["a"], "change": [], "fee": True},
        "not object": [1, 2],
    }
    for name, r in bad.items():
        assert validate_tx(w, outs, r) is not None, name
    # oversized: more inputs than max_tx_size allows
    big = {"u%d" % i: ADA for i in range(200)}
    r = {"inputs": list(big), "change": [], "fee": 200 * ADA - 3 * ADA}
    assert validate_tx(big, [3 * ADA], r) == "size %d over max" % tx_size(200, 1)
    # largest-first on the small wallet
    r = largest_first(w, outs)
    assert r == good, r
    assert largest_first({"x": ADA}, [2 * ADA]) is None
    # fold: leftover below min_ada goes into the fee
    fw = {"x": 3 * ADA + fee - 1 + 500_000}
    r = largest_first(fw, outs)
    assert r is not None and r["change"] == [] and validate_tx(fw, outs, r) is None
    # generated scenarios
    a = gen_scenarios(7)
    assert a == gen_scenarios(7) and gen_scenarios(8) != a
    assert len(a) == N_SCENARIOS
    for seed in range(5):
        for sc in gen_scenarios(seed):
            assert len(sc) == N_EVENTS + 1
            assert 60 <= len(sc[0]["utxos"]) <= 120
            assert all(u["lovelace"] >= int(1.5 * ADA) for u in sc[0]["utxos"])
            txs, fees = simulate_largest_first(sc)
            assert txs > 150
    print("sim self-test ok")


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        self_test()
    else:
        sys.exit("usage: sim.py --self-test")
