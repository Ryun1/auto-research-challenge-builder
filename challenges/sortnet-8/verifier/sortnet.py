"""8-input sorting network check. Reads candidate/network.txt as data.

One comparator per line, "i j" with 0 <= i < j < 8: after it, wire i holds
the smaller value. The network must sort every input: all 256 0/1 inputs
(sufficient by the 0-1 principle) plus 50,000 CB_SEED-salted integer
inputs (the ECDSA.fail lesson: never trust a small fixed set alone).
Score = comparator count (depth reported too).
"""
import json
import os
import random
import sys

N = 8
MAX_BYTES = 4096
SALTED = 50_000


def parse(path):
    try:
        with open(path, "rb") as f:
            raw = f.read(MAX_BYTES + 1)
    except OSError as e:
        return None, f"cannot read {path}: {e}"
    if len(raw) > MAX_BYTES:
        return None, f"network.txt over {MAX_BYTES} bytes"
    net = []
    try:
        lines = raw.decode("ascii").splitlines()
    except UnicodeDecodeError:
        return None, "network.txt must be ASCII"
    for k, line in enumerate(lines, 1):
        if not line.strip():
            continue
        parts = line.split()
        if len(parts) != 2 or not all(p.isdigit() for p in parts):
            return None, f"line {k}: expected two wire numbers"
        i, j = map(int, parts)
        if not (0 <= i < j < N):
            return None, f"line {k}: need 0 <= i < j < {N}"
        net.append((i, j))
    return net, ""


def apply(net, xs):
    xs = list(xs)
    for i, j in net:
        if xs[i] > xs[j]:
            xs[i], xs[j] = xs[j], xs[i]
    return xs


def depth(net):
    level = [0] * N
    for i, j in net:
        level[i] = level[j] = max(level[i], level[j]) + 1
    return max(level, default=0)


def check(net):
    for bits in range(1 << N):
        xs = [(bits >> b) & 1 for b in range(N)]
        if apply(net, xs) != sorted(xs):
            return f"fails 0/1 input {xs}"
    rng = random.Random(int(os.environ.get("CB_SEED", "0")))
    for _ in range(SALTED):
        xs = [rng.randrange(-(1 << 31), 1 << 31) for _ in range(N)]
        if apply(net, xs) != sorted(xs):
            return f"fails salted input {xs}"
    return ""


def main():
    net, err = parse("candidate/network.txt")
    if net is not None:
        err = check(net)
    ok = net is not None and not err
    if sys.argv[1] == "verify":
        detail = f"sorts all {1 << N} 0/1 + {SALTED} salted inputs with {len(net)} comparators" if ok else err
        print(json.dumps({"ok": ok, "detail": detail}))
        sys.exit(0 if ok else 1)
    if not ok:
        print(err, file=sys.stderr)
        sys.exit(1)
    print(json.dumps({"metrics": {"comparators": len(net), "depth": depth(net)}}))


if __name__ == "__main__":
    main()
