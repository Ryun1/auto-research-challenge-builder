#!/usr/bin/env python3
"""Scenario generator, transaction validator and the multi-asset
largest-first reference for contract wallet-multiasset@1. Trusted judge
code, stdlib only. Sizes, fees and min-ada come from model.py.

`python3 sim.py --self-test` runs the unit tests in this directory.
"""
import hashlib
import math
import os
import random
import re
import sys
import unittest
from typing import NamedTuple

from cbor import encode
from model import (ADA, PARAMS, Datum, Output, Value, min_ada, min_fee, min_lovelace,
                   tx_size, utxo_ref, value_size, wallet_address)

N_SCENARIOS = 8
N_EVENTS = 300
FEE_BUDGET = 1 * ADA  # fundability assumes every earlier tx cost at most this
# Deposit UTxOs are 70% ADA-only, 24% 1-3 assets, 6% bundles of 10-30
# assets. The plan's 8% x 10-60 piled ~1000 assets into the wallet and the
# reference outgrew max_tx_size on 4/128 scenarios; 6% x 10-30 serves all
# of seeds 0-47 with the largest reference tx at 11.9 KB.
BUNDLE_P = 0.06
BUNDLE_N = (10, 30)

U64 = 2 ** 64
_HEX = re.compile(r"(?:[0-9a-f]{2})*")


class Utxo(NamedTuple):
    ref: tuple  # (txid bytes, index)
    addr: int  # wallet key
    value: Value
    datum: Datum


def _canon(b: bytes):
    return (len(b), b)


def _is_int(x):
    return isinstance(x, int) and not isinstance(x, bool)


# ------------------------------------------------------------------ parsing

def _hex(s, lo, hi):
    """Lowercase even-length hex decoding to lo..hi bytes, else ValueError."""
    if not isinstance(s, str) or not _HEX.fullmatch(s) or not lo <= len(s) // 2 <= hi:
        raise ValueError("bad hex")
    return bytes.fromhex(s)


def parse_value(j) -> Value:
    if not isinstance(j, dict) or "lovelace" not in j or set(j) - {"lovelace", "assets"}:
        raise ValueError("bad value")
    coin = j["lovelace"]
    if not _is_int(coin) or not 0 <= coin < U64:
        raise ValueError("bad lovelace")
    aj = j.get("assets", {})
    if not isinstance(aj, dict):
        raise ValueError("bad assets")
    assets = {}
    for pol, names in aj.items():
        if not isinstance(names, dict) or not names:
            raise ValueError("bad asset map")
        m = {}
        for name, qty in names.items():
            if not _is_int(qty) or not 0 < qty < U64:
                raise ValueError("bad quantity")
            m[_hex(name, 0, 32)] = qty
        assets[_hex(pol, 28, 28)] = m
    return (coin, assets)


def value_json(v: Value) -> dict:
    coin, assets = v
    return {"lovelace": coin,
            "assets": {p.hex(): {n.hex(): q for n, q in names.items()}
                       for p, names in assets.items()}}


def parse_datum(j) -> Datum:
    if not isinstance(j, dict) or len(j) != 1:
        raise ValueError("bad datum")
    if "inline" in j:
        return ("inline", _hex(j["inline"], 1, 1 << 20))
    if "hash" in j:
        return ("hash", _hex(j["hash"], 32, 32))
    raise ValueError("bad datum")


def datum_json(d: Datum):
    return {d[0]: d[1].hex()}


def parse_output(j) -> Output:
    if not isinstance(j, dict) or not {"address", "value"} <= set(j) \
            or set(j) - {"address", "value", "datum"}:
        raise ValueError("bad output")
    addr = _hex(j["address"], 29, 57)
    if len(addr) not in (29, 57):
        raise ValueError("bad address")
    datum = parse_datum(j["datum"]) if "datum" in j else None
    return Output(addr, parse_value(j["value"]), datum)


def add_deposit(wallet, event):
    for u in event["utxos"]:
        datum = parse_datum(u["datum"]) if "datum" in u else None
        wallet[u["id"]] = Utxo(utxo_ref(u["id"], u["index"]), u["addr"],
                               parse_value(u["value"]), datum)


# ---------------------------------------------------------- asset helpers

def _flat(assets):
    return {(p, n): q for p, names in assets.items() for n, q in names.items()}


def _nest(flat):
    out = {}
    for (p, n), q in flat.items():
        out.setdefault(p, {})[n] = q
    return out


def _add_into(acc, assets, sign=1):
    for p, names in assets.items():
        for n, q in names.items():
            acc[(p, n)] = acc.get((p, n), 0) + sign * q


# ---------------------------------------------------------------- validator

def validate_tx(wallet, pay_outputs, reply, p=PARAMS):
    """Return None when `reply` is a valid spend of `wallet` paying
    `pay_outputs` (list of model.Output), else a short reason."""
    if not isinstance(reply, dict):
        return "reply is not an object"
    ins, change, fee = reply.get("inputs"), reply.get("change"), reply.get("fee")
    if not isinstance(ins, list) or not isinstance(change, list):
        return "inputs and change must be lists"
    if not _is_int(fee) or not 0 <= fee < U64:
        return "fee is not a non-negative integer"
    if not ins:
        return "no inputs"
    if not all(isinstance(i, str) for i in ins):
        return "input ids must be strings"
    if len(set(ins)) != len(ins):
        return "duplicate input"
    if any(i not in wallet for i in ins):
        return "unknown input"
    # every input encodes in >= 36 bytes and every change output (57-byte
    # address) in >= 60: a cheap lower bound before parsing anything large
    if 36 * len(ins) + 60 * len(change) > p["max_tx_size"]:
        return "size over max (too many inputs or outputs)"
    outs = []
    for c in change:
        if not isinstance(c, dict) or set(c) != {"addr", "value"}:
            return "change entry must be {addr, value}"
        a = c["addr"]
        if not _is_int(a) or not 0 <= a < p["keys"]:
            return "bad change addr"
        try:
            v = parse_value(c["value"])
        except ValueError:
            return "bad value"
        outs.append(Output(wallet_address(a), v))
    used = [wallet[i] for i in ins]
    if sum(u.value[0] for u in used) != (sum(o.value[0] for o in pay_outputs)
                                         + sum(o.value[0] for o in outs) + fee):
        return "imbalance"
    bal = {}
    for u in used:
        _add_into(bal, u.value[1])
    for o in pay_outputs + outs:
        _add_into(bal, o.value[1], -1)
    if any(bal.values()):
        return "asset imbalance"
    n_wit = len({u.addr for u in used})
    size = tx_size([u.ref for u in used], pay_outputs + outs, fee, n_wit)
    if size > p["max_tx_size"]:
        return "size %d over max" % size
    if fee < min_fee(size, p):
        return "fee below min_fee"
    for o in outs:
        if value_size(o.value) > p["max_val_size"]:
            return "change over max_val_size"
        if o.value[0] < min_ada(o, p):
            return "change below min_ada"
    return None


# ------------------------------------------------------ largest-first

_COIN_MAX = U64 - 1  # widest coin, so a group fits whatever lovelace it ends up with


def _split(assets, p=PARAMS):
    """Greedy partition of leftover assets into change values that each fit
    max_val_size: whole policies in canonical order, names only when one
    policy alone is too big."""
    def fits(g):
        return value_size((_COIN_MAX, g)) <= p["max_val_size"]

    groups = [{}]
    for pol in sorted(assets, key=_canon):
        names = assets[pol]
        trial = {**groups[-1], pol: names}
        if fits(trial):
            groups[-1] = trial
        elif fits({pol: names}):
            groups.append({pol: names})
        else:
            for n in sorted(names, key=_canon):
                cur = groups[-1]
                trial = {**cur, pol: {**cur.get(pol, {}), n: names[n]}}
                if fits(trial):
                    groups[-1] = trial
                else:
                    groups.append({pol: {n: names[n]}})
    return [g for g in groups if g]


def _build(wallet, picked, pay_outputs, p):
    """Try to finish a tx spending `picked`. Returns (reply, size) where
    reply is None when the inputs don't cover payment, fee and min-ada."""
    used = [wallet[u] for u in picked]
    refs = [u.ref for u in used]
    n_wit = len({u.addr for u in used})
    spare = sum(u.value[0] for u in used) - sum(o.value[0] for o in pay_outputs)
    left = {}
    for u in used:
        _add_into(left, u.value[1])
    for o in pay_outputs:
        _add_into(left, o.value[1], -1)
    left = {k: q for k, q in left.items() if q}
    addr0 = wallet_address(0)

    def reply(change_vals, fee):
        return {"inputs": list(picked),
                "change": [{"addr": 0, "value": value_json(v)} for v in change_vals],
                "fee": fee}

    if left:
        groups = _split(_nest(left), p)
        mins = [min_lovelace(addr0, g, None, p) for g in groups]
        fee = 0
        while True:
            c0 = spare - fee - sum(mins[1:])
            if c0 < mins[0]:
                vals = [(m, g) for m, g in zip(mins, groups)]
                return None, tx_size(refs, pay_outputs + [Output(addr0, v) for v in vals],
                                     0, n_wit)
            vals = [(c0, groups[0])] + list(zip(mins[1:], groups[1:]))
            size = tx_size(refs, pay_outputs + [Output(addr0, v) for v in vals], fee, n_wit)
            f = min_fee(size, p)
            if f <= fee:
                return reply(vals, fee), size
            fee = f

    # ADA only: one change at key 0, else fold the leftover into the fee
    fee = 0
    while spare - fee >= 0:
        v = (spare - fee, {})
        size = tx_size(refs, pay_outputs + [Output(addr0, v)], fee, n_wit)
        f = min_fee(size, p)
        if f <= fee:
            if v[0] >= min_ada(Output(addr0, v), p):
                return reply([v], fee), size
            break
        fee = f
    size = tx_size(refs, pay_outputs, max(spare, 0), n_wit)
    if spare >= min_fee(size, p):
        return reply([], spare), size
    return None, size


def reference(wallet, pay_outputs, p=PARAMS):
    """The judge's reference. Returns a reply dict, or None if the wallet
    cannot pay within max_tx_size."""
    need = {}
    for o in pay_outputs:
        _add_into(need, o.value[1])
    picked, have = [], {}

    def take(uid):
        picked.append(uid)
        _add_into(have, wallet[uid].value[1])

    for key in sorted(need, key=lambda k: (_canon(k[0]), _canon(k[1]))):
        pol, name = key
        holders = sorted((u for u in wallet if u not in picked
                          and wallet[u].value[1].get(pol, {}).get(name, 0) > 0),
                         key=lambda u: (-wallet[u].value[1][pol][name], u))
        for uid in holders:
            if have.get(key, 0) >= need[key]:
                break
            take(uid)
        if have.get(key, 0) < need[key]:
            return None

    rest = sorted((u for u in wallet if u not in picked),
                  key=lambda u: (-wallet[u].value[0], u))
    if picked:
        r, size = _build(wallet, picked, pay_outputs, p)
        if size > p["max_tx_size"]:
            return None
        if r is not None:
            return r
    for uid in rest:
        take(uid)
        r, size = _build(wallet, picked, pay_outputs, p)
        if size > p["max_tx_size"]:
            return None
        if r is not None:
            return r
    return None


def apply_tx(wallet, pay_id, n_pay_outputs, reply):
    for i in reply["inputs"]:
        del wallet[i]
    for k, c in enumerate(reply["change"]):
        uid = "%s#%d" % (pay_id, k)
        wallet[uid] = Utxo(utxo_ref(uid, n_pay_outputs + k), c["addr"],
                           parse_value(c["value"]), None)


def locked_ada(wallet) -> int:
    """Σ min_ada over the wallet's UTxOs, in lovelace."""
    return sum(min_ada(Output(wallet_address(u.addr), u.value, u.datum)) for u in wallet.values())


def ada_dust(wallet) -> int:
    """UTxOs whose free ADA (lovelace above their own min_ada) is under 1 ADA.

    Counts token and datum UTxOs too, so moving a small ADA amount into a token
    UTxO does not hide it. An ADA-only UTxO at a wallet key counts below about
    1.97 ADA (min_ada 969,750 + 1 ADA).
    """
    return sum(1 for u in wallet.values()
               if u.value[0] - min_ada(Output(wallet_address(u.addr), u.value, u.datum)) < ADA)


def simulate_reference(scenario):
    """Run a scenario with the reference; returns (txs, fees) or raises."""
    wallet = {}
    txs = fees = 0
    for e in scenario:
        if e["t"] == "deposit":
            add_deposit(wallet, e)
            continue
        outs = [parse_output(o) for o in e["outputs"]]
        r = reference(wallet, outs)
        if r is None:
            raise AssertionError("unfundable " + e["id"])
        err = validate_tx(wallet, outs, r)
        if err:
            raise AssertionError("reference invalid on %s: %s" % (e["id"], err))
        apply_tx(wallet, e["id"], len(outs), r)
        txs += 1
        fees += r["fee"]
    return txs, fees


# ---------------------------------------------------------------- generator

def _lognorm(rng, median_ada, sigma):
    return int(rng.lognormvariate(math.log(median_ada * ADA), sigma))


def _rand_bytes(rng, n):
    return rng.getrandbits(8 * n).to_bytes(n, "big") if n else b""


def _universe(rng):
    """5-30 policies: fungibles (1-3 names) and NFT collections."""
    pols = []
    for _ in range(rng.randint(5, 30)):
        pid = _rand_bytes(rng, 28)
        if rng.random() < 0.5:
            names = sorted({_rand_bytes(rng, rng.randint(0, 32)) for _ in range(rng.randint(1, 3))})
            pols.append({"id": pid, "nft": False, "names": names})
        else:
            pols.append({"id": pid, "nft": True, "len": rng.randint(4, 32), "seen": set()})
    return pols


def _draw_asset(rng, pol):
    if not pol["nft"]:
        return rng.choice(pol["names"]), int(10 ** rng.uniform(3, 12))
    while True:
        name = _rand_bytes(rng, pol["len"])
        if name not in pol["seen"]:
            pol["seen"].add(name)
            return name, 1


def _draw_assets(rng, uni):
    r = rng.random()
    if r < 0.70:
        return {}
    if r < 1 - BUNDLE_P:
        n, pols = rng.randint(1, 3), uni
    else:  # a bundle from a few policies
        n, pols = rng.randint(*BUNDLE_N), rng.sample(uni, min(len(uni), rng.randint(1, 3)))
    out = {}
    for _ in range(n):
        pol = rng.choice(pols)
        name, qty = _draw_asset(rng, pol)
        m = out.setdefault(pol["id"], {})
        m[name] = min(U64 - 1, m.get(name, 0) + qty)
    return out


def _deposit_datum(rng):
    r = rng.random()
    if r < 0.10:
        return ("inline", encode(_rand_bytes(rng, rng.randint(16, 1024))))
    if r < 0.15:
        return ("hash", _rand_bytes(rng, 32))
    return None


def _deposit_utxo(rng, uni, initial):
    addr = rng.randrange(PARAMS["keys"])
    assets = _draw_assets(rng, uni)
    datum = _deposit_datum(rng)
    floor = min_lovelace(wallet_address(addr), assets, datum)
    if assets:
        coin = floor + (0 if rng.random() < 0.5 else rng.randint(0, 2 * ADA))
    elif initial:
        coin = max(int(1.5 * ADA), _lognorm(rng, 50, 1.0))
    elif rng.random() < 0.7:
        coin = max(ADA, _lognorm(rng, 30, 1.2))
    else:
        coin = rng.randint(1 * ADA, 3 * ADA)  # reward-like
    return addr, (max(coin, floor), assets), datum


def _pay_address(rng, datum):
    if datum:
        return b"\x11" + _rand_bytes(rng, 56)
    if rng.random() < 0.7:
        return b"\x01" + _rand_bytes(rng, 56)
    return b"\x61" + _rand_bytes(rng, 28)


def _pay_datum(rng):
    if rng.random() < 0.5:
        return ("inline", encode(_rand_bytes(rng, rng.randint(16, 512))))
    return ("hash", _rand_bytes(rng, 32))


def gen_scenario(rng):
    """One scenario: a list of events. Event 0 is the initial deposit."""
    uni = _universe(rng)
    ev = []
    held = {}  # (policy, name) -> qty the wallet holds
    state = {"bal": 0}

    def deposit(n, initial=False):
        i = len(ev)
        utxos = []
        for k in range(n):
            addr, v, datum = _deposit_utxo(rng, uni, initial)
            u = {"id": "d%d#%d" % (i, k), "index": k, "addr": addr, "value": value_json(v)}
            if datum:
                u["datum"] = datum_json(datum)
            utxos.append(u)
            state["bal"] += v[0]
            _add_into(held, v[1])
        ev.append({"t": "deposit", "utxos": utxos})

    def reserve():
        """Lovelace to hold every policy's whole holding in its own change."""
        nested = _nest(held)
        return sum(min_lovelace(wallet_address(0), {p: names}, None)
                   for p, names in nested.items())

    deposit(rng.randint(60, 120), initial=True)
    while len(ev) < N_EVENTS + 1:
        if rng.random() < 0.3:
            deposit(rng.randint(1, 5))
            continue
        n_out = rng.randint(1, 3)
        per_out = [{} for _ in range(n_out)]
        if held and rng.random() < 0.2:
            kinds = list(held)
            for key in rng.sample(kinds, min(len(kinds), rng.randint(1, 3))):
                have = held[key]
                qty = rng.randint(1, max(1, have // 2))
                per_out[rng.randrange(n_out)].setdefault(key[0], {})[key[1]] = qty
        datum_at = rng.randrange(n_out) if rng.random() < 0.1 else None
        outputs = []
        for k in range(n_out):
            datum = _pay_datum(rng) if k == datum_at else None
            address = _pay_address(rng, datum)
            assets = per_out[k]
            floor = min_lovelace(address, assets, datum)
            coin = 0 if assets and rng.random() < 0.5 else max(ADA, _lognorm(rng, 20, 1.0))
            outputs.append(Output(address, (max(coin, floor), assets), datum))
        need = sum(o.value[0] for o in outputs)
        for o in outputs:
            _add_into(held, o.value[1], -1)
        for key in [k for k, q in held.items() if q == 0]:
            del held[key]
        while state["bal"] - reserve() < 3 * (need + FEE_BUDGET):
            deposit(rng.randint(1, 5))
        i = len(ev)
        pay = {"t": "pay", "id": "p%d" % i, "outputs": []}
        for o in outputs:
            j = {"address": o.address.hex(), "value": value_json(o.value)}
            if o.datum:
                j["datum"] = datum_json(o.datum)
            pay["outputs"].append(j)
        ev.append(pay)
        state["bal"] -= need + FEE_BUDGET
    return ev[:N_EVENTS + 1]  # any prefix is fundable


class HashRandom(random.Random):
    """A random source from SHA-256 in counter mode over (seed, k, counter), so
    the values a participant sees reveal nothing about later ones (unlike
    MT19937, whose state can be recovered from its outputs)."""

    def __init__(self, seed, k):
        self._key = ("%d:%d:" % (seed, k)).encode()
        self._ctr = 0
        self._pool = 0
        self._bits = 0
        super().__init__(0)

    def seed(self, a=None, version=2):  # random.Random.__init__ calls this
        pass

    def getstate(self):
        return (self._key, self._ctr, self._pool, self._bits)

    def setstate(self, state):
        self._key, self._ctr, self._pool, self._bits = state

    def _block(self):
        h = hashlib.sha256(self._key + str(self._ctr).encode()).digest()
        self._ctr += 1
        return int.from_bytes(h, "big")

    def getrandbits(self, k):
        if k < 0:
            raise ValueError("number of bits must be non-negative")
        out, n = 0, 0
        while n < k:
            out |= self._block() << n
            n += 256
        return out & ((1 << k) - 1)

    def random(self):
        return self.getrandbits(53) / (1 << 53)


def gen_scenarios(seed):
    return [gen_scenario(HashRandom(seed, k)) for k in range(N_SCENARIOS)]


# --------------------------------------------------------------- self-test

def self_test():
    here = os.path.dirname(os.path.abspath(__file__))
    suite = unittest.defaultTestLoader.discover(here, pattern="test_*.py", top_level_dir=here)
    if not unittest.TextTestRunner(verbosity=1).run(suite).wasSuccessful():
        sys.exit(1)
    print("sim self-test ok")


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        self_test()
    else:
        sys.exit("usage: sim.py --self-test")
