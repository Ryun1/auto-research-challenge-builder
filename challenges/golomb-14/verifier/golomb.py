"""Golomb ruler check. Reads candidate/ruler.txt as data; never executes it.

A ruler is 14 distinct non-negative integers starting at 0, strictly
increasing, with all pairwise differences distinct. Score = last mark.
"""
import json
import sys

MARKS = 14
MAX_BYTES = 4096


def check(path):
    try:
        with open(path, "rb") as f:
            raw = f.read(MAX_BYTES + 1)
    except OSError as e:
        return None, f"cannot read {path}: {e}"
    if len(raw) > MAX_BYTES:
        return None, f"ruler.txt over {MAX_BYTES} bytes"
    try:
        marks = [int(tok) for tok in raw.decode("ascii").split()]
    except (UnicodeDecodeError, ValueError):
        return None, "ruler.txt must be whitespace-separated decimal integers"
    if len(marks) != MARKS:
        return None, f"need exactly {MARKS} marks, got {len(marks)}"
    if marks[0] != 0 or any(b <= a for a, b in zip(marks, marks[1:])):
        return None, "marks must start at 0 and strictly increase"
    diffs = [b - a for i, a in enumerate(marks) for b in marks[i + 1:]]
    if len(set(diffs)) != len(diffs):
        return None, "two pairs of marks share a difference"
    return marks, f"valid {MARKS}-mark Golomb ruler, length {marks[-1]}"


def main():
    mode = sys.argv[1]
    marks, detail = check("candidate/ruler.txt")
    if mode == "verify":
        print(json.dumps({"ok": marks is not None, "detail": detail}))
        sys.exit(0 if marks else 1)
    if marks is None:
        print(detail, file=sys.stderr)
        sys.exit(1)
    print(json.dumps({"metrics": {"length": marks[-1]}}))


if __name__ == "__main__":
    main()
