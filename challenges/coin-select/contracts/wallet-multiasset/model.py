"""Byte model for wallet-multiasset@1.

This file is the definition of size, fee and min-ada for wallet-multiasset@1; solvers may copy it.
"""
import hashlib
from typing import NamedTuple, Optional, Tuple

from cbor import Tag, encode

ADA = 1_000_000


def wallet_address(i: int) -> bytes:
    """57-byte base address for wallet key i (payment hash == stake hash)."""
    h = hashlib.sha256(b"key%d" % i).digest()[:28]
    return b"\x01" + h * 2


PARAMS = {
    "min_fee_a": 44,
    "min_fee_b": 155381,
    "coins_per_utxo_byte": 4310,
    "max_tx_size": 16384,
    "max_val_size": 5000,
    "keys": 8,
    "addresses": [wallet_address(i).hex() for i in range(8)],
}

# (lovelace, {policy(28 bytes): {name(0-32 bytes): qty > 0}})
Value = Tuple[int, dict]
# None | ("inline", datum CBOR bytes) | ("hash", 32 bytes)
Datum = Optional[Tuple[str, bytes]]


class Output(NamedTuple):
    address: bytes
    value: Value
    datum: Datum = None


def _canon_key(b: bytes):
    return (len(b), b)


def value_cbor(v: Value, order: str = "canonical"):
    coin, assets = v
    if not assets:
        return coin
    if order == "canonical":
        pols = sorted(assets, key=_canon_key)
        m = {p: {n: assets[p][n] for n in sorted(assets[p], key=_canon_key)} for p in pols}
    elif order == "given":
        m = {p: dict(names) for p, names in assets.items()}
    else:
        raise ValueError("unknown order %r" % order)
    return [coin, m]


def output_cbor(o: Output, form: str = "judge", order: str = "canonical"):
    val = value_cbor(o.value, order)
    kind = o.datum[0] if o.datum else None
    if form == "judge":
        form = "map" if kind == "inline" else "array"
    if form == "array":
        if kind == "inline":
            raise ValueError("array form cannot carry an inline datum")
        out = [o.address, val]
        if kind == "hash":
            out.append(o.datum[1])
        return out
    if form == "map":
        m = {0: o.address, 1: val}
        if kind == "inline":
            m[2] = [1, Tag(24, o.datum[1])]
        elif kind == "hash":
            m[2] = [0, o.datum[1]]
        return m
    raise ValueError("unknown form %r" % form)


def output_size(o: Output) -> int:
    return len(encode(output_cbor(o)))


def value_size(v: Value) -> int:
    return len(encode(value_cbor(v)))


def min_ada(o: Output, p=PARAMS) -> int:
    return (160 + output_size(o)) * p["coins_per_utxo_byte"]


def min_lovelace(address: bytes, assets, datum, p=PARAMS) -> int:
    l = 0
    while True:
        need = min_ada(Output(address, (l, assets), datum), p)
        if need <= l:
            return l
        l = need


def tx_cbor(inputs, outputs, fee, n_witnesses, tag_sets=False, extra_body=(), aux=None,
            tag_witnesses=None):
    """The tx as CBOR items. tag_sets tags the input set and, unless
    tag_witnesses says otherwise, the witness set."""
    ins = [[txid, idx] for txid, idx in inputs]
    wits = [[b"\x00" * 32, b"\x00" * 64] for _ in range(n_witnesses)]
    if tag_witnesses is None:
        tag_witnesses = tag_sets
    if tag_sets:
        ins = Tag(258, ins)
    if tag_witnesses:
        wits = Tag(258, wits)
    body = {0: ins, 1: list(outputs), 2: fee}
    for k, raw in extra_body:
        body[k] = raw
    return [body, {0: wits}, True, aux]


def tx_size(inputs, outputs, fee, n_witnesses) -> int:
    """The ledger's tx size, used for min_fee and max_tx_size.

    It is the judge-form tx's CBOR length less one: the ledger measures the
    3-item [body, witness_set, aux] (Alonzo toCBORForSizeComputation, kept in
    Conway), so the is_valid byte is not counted. Mainnet agrees: of 3,892
    fixture-eligible txs, 537 pay exactly min_fee(len(cbor) - 1) and none less.
    """
    return len(encode(tx_cbor(inputs, [output_cbor(o) for o in outputs], fee, n_witnesses))) - 1


def min_fee(size: int, p=PARAMS) -> int:
    return p["min_fee_a"] * size + p["min_fee_b"]


def utxo_ref(uid: str, index: int):
    return (hashlib.sha256(uid.split("#")[0].encode()).digest(), index)
