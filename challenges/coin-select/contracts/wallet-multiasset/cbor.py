"""Minimal canonical CBOR encoder/decoder (stdlib only)."""
from typing import NamedTuple


class Tag(NamedTuple):
    tag: int
    value: object


class Raw(NamedTuple):
    data: bytes


class NonCanonical(ValueError):
    pass


def _head(major, n):
    if n < 0:
        raise ValueError("negative head argument")
    m = major << 5
    if n < 24:
        return bytes([m | n])
    if n < 1 << 8:
        return bytes([m | 24, n])
    if n < 1 << 16:
        return bytes([m | 25]) + n.to_bytes(2, "big")
    if n < 1 << 32:
        return bytes([m | 26]) + n.to_bytes(4, "big")
    if n < 1 << 64:
        return bytes([m | 27]) + n.to_bytes(8, "big")
    raise ValueError("integer too large")


def encode(obj):
    if isinstance(obj, Raw):
        return obj.data
    if obj is True:
        return b"\xf5"
    if obj is False:
        return b"\xf4"
    if obj is None:
        return b"\xf6"
    if isinstance(obj, int):
        return _head(0, obj) if obj >= 0 else _head(1, -1 - obj)
    if isinstance(obj, (bytes, bytearray)):
        return _head(2, len(obj)) + bytes(obj)
    if isinstance(obj, str):
        b = obj.encode("utf-8")
        return _head(3, len(b)) + b
    if isinstance(obj, Tag):
        return _head(6, obj.tag) + encode(obj.value)
    if isinstance(obj, (list, tuple)):
        return _head(4, len(obj)) + b"".join(encode(x) for x in obj)
    if isinstance(obj, dict):
        return _head(5, len(obj)) + b"".join(encode(k) + encode(v) for k, v in obj.items())
    raise ValueError(f"cannot encode {type(obj).__name__}")


def _read_head(data, off):
    if off >= len(data):
        raise ValueError("truncated CBOR")
    ib = data[off]
    major, info = ib >> 5, ib & 31
    off += 1
    if info < 24:
        return major, info, off
    if info > 27:
        if info == 31:
            raise NonCanonical("indefinite-length item")
        raise ValueError("reserved additional info")
    width = 1 << (info - 24)
    if off + width > len(data):
        raise ValueError("truncated CBOR")
    n = int.from_bytes(data[off:off + width], "big")
    if n < (24, 1 << 8, 1 << 16, 1 << 32)[info - 24]:
        raise NonCanonical("non-minimal head")
    return major, n, off + width


def _hashable(k):
    if isinstance(k, list):
        return tuple(_hashable(x) for x in k)
    if isinstance(k, dict):
        return tuple((_hashable(a), _hashable(b)) for a, b in k.items())
    return k


def _map_entries(data, off, n):
    entries, seen = [], set()
    for _ in range(n):
        key, off = decode(data, off)
        hk = (type(key) is bool, _hashable(key))
        if hk in seen:
            raise NonCanonical("duplicate map key")
        seen.add(hk)
        vs = off
        _, off = decode(data, off)
        entries.append((key, vs, off))
    return entries, off


def decode(data, offset=0):
    major, n, off = _read_head(data, offset)
    if major == 0:
        return n, off
    if major == 1:
        return -1 - n, off
    if major in (2, 3):
        end = off + n
        if end > len(data):
            raise ValueError("truncated CBOR")
        raw = bytes(data[off:end])
        return (raw if major == 2 else raw.decode("utf-8")), end
    if major == 4:
        out = []
        for _ in range(n):
            v, off = decode(data, off)
            out.append(v)
        return out, off
    if major == 5:
        entries, end = _map_entries(data, off, n)
        return {k: decode(data, s)[0] for k, s, _ in entries}, end
    if major == 6:
        v, off = decode(data, off)
        return Tag(n, v), off
    # major 7
    ib = data[offset] & 31
    if ib == 20:
        return False, off
    if ib == 21:
        return True, off
    if ib == 22:
        return None, off
    raise ValueError("unsupported simple/float item")


def decode_map_spans(data, offset):
    major, n, off = _read_head(data, offset)
    if major != 5:
        raise ValueError("not a map")
    return _map_entries(data, off, n)[0]
