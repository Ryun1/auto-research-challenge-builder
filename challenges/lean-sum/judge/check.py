"""lean-sum judge. The candidate is Lean source, so it is elaborated only
here, in verify (sandboxed in the pipeline: nobody, no network, temp dir).

1. Lexical policy on candidate/Submission.lean before Lean sees it.
2. Compile judge/Spec.lean, then the candidate, into a temp dir.
3. The judge restates the theorem against Spec.spec and prints its axioms:
   only propext, Classical.choice, Quot.sound are allowed (catches sorry,
   native_decide and any custom axiom at the kernel level).
4. CB_SEED-salted executable vectors as a falsification check.
Score = non-whitespace characters of the submission (minimise).
"""
import json
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

SUB = "candidate/Submission.lean"
MAX_BYTES = 16384
ALLOWED_AXIOMS = {"propext", "Classical.choice", "Quot.sound"}
# Commands and attributes that run code at elaboration time, change the
# kernel's rules, or swap compiled code under a proof.
BANNED = [
    "#eval", "#exit", "run_cmd", "run_elab", "run_meta", "run_tac", "elab", "elab_rules",
    "macro", "macro_rules", "syntax", "declare_syntax_cat", "initialize", "builtin_initialize",
    "set_option", "native_decide", "sorry", "admit", "axiom", "implemented_by", "extern",
    "csimp", "export", "unsafe", "partial", "opaque", "instance", "class", "notation",
    "infix", "infixl", "infixr", "prefix", "postfix", "open",
]


def strip(src):
    """Blank comments (--, nested /- -/) and string/char literals."""
    out, i, n = [], 0, len(src)
    while i < n:
        if src.startswith("--", i):
            while i < n and src[i] != "\n":
                i += 1
        elif src.startswith("/-", i):
            depth = 0
            while i < n:
                if src.startswith("/-", i):
                    depth, i = depth + 1, i + 2
                elif src.startswith("-/", i):
                    depth, i = depth - 1, i + 2
                    if depth == 0:
                        break
                else:
                    out.append("\n" if src[i] == "\n" else " ")
                    i += 1
        elif src[i] == '"':
            i += 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
            i += 1
            out.append(" ")
        else:
            out.append(src[i])
            i += 1
    return "".join(out)


def policy(src):
    code = strip(src)
    errors = []
    for k, line in enumerate(code.splitlines(), 1):
        if line.lstrip().startswith("import") and line.split() != ["import", "Spec"]:
            errors.append(f"line {k}: only `import Spec` is allowed")
        for tok in BANNED:
            if re.search(r"(?<![\w.'«])" + re.escape(tok) + r"(?![\w'!?»])", line):
                errors.append(f"line {k}: `{tok}` not allowed")
    if "«" in code:
        errors.append("guillemet identifiers are not allowed")
    return errors


def lean():
    try:
        with open(".lean-bin") as f:
            return os.path.join(f.read().strip(), "lean")
    except OSError:
        return shutil.which("lean") or os.path.expanduser("~/.elan/bin/lean")


def run(args, cwd, env=None):
    p = subprocess.run(args, cwd=cwd, capture_output=True, text=True, timeout=600,
                       env={**os.environ, **(env or {})})
    return p.returncode, (p.stdout + p.stderr)[-1500:]


def verify():
    try:
        files = sorted(os.listdir("candidate"))
        src = open(SUB, encoding="utf-8").read(MAX_BYTES + 1)
    except (OSError, UnicodeDecodeError) as e:
        return f"cannot read {SUB}: {e}"
    if files != ["Submission.lean"]:
        return f"candidate/ must hold exactly Submission.lean, has {files}"
    if len(src.encode()) > MAX_BYTES:
        return f"Submission.lean over {MAX_BYTES} bytes"
    errors = policy(src)
    if errors:
        return "policy: " + "; ".join(errors[:5])
    tmp = tempfile.mkdtemp(prefix="lean-sum-")
    shutil.copy("judge/Spec.lean", tmp)
    shutil.copy(SUB, tmp)
    L, env = lean(), {"LEAN_PATH": tmp}
    for f in ("Spec", "Submission"):
        code, out = run([L, "-o", f"{f}.olean", f"{f}.lean"], tmp, env)
        if code:
            return f"{f}.lean does not compile: {out}"
    rng = random.Random(int(os.environ.get("CB_SEED", "0")))
    vectors = [0, 1, 2] + [rng.randrange(20000) for _ in range(50)]
    check = (
        "import Submission\n"
        "example : ∀ n, sumTo n = Spec.spec n := sumTo_correct\n"
        "#print axioms sumTo_correct\n"
        f"#eval List.all {vectors} (fun n => Nat.beq (sumTo n) (Spec.spec n))\n"
    )
    with open(os.path.join(tmp, "Check.lean"), "w") as f:
        f.write(check)
    code, out = run([L, "Check.lean"], tmp, env)
    shutil.rmtree(tmp, ignore_errors=True)
    if code:
        return f"the judge's statement does not check: {out}"
    m = re.search(r"depends on axioms: \[([^\]]*)\]", out)
    axioms = {a.strip() for a in m.group(1).split(",")} if m else set()
    if "does not depend on any axioms" not in out and not m:
        return f"no axiom report: {out}"
    if axioms - ALLOWED_AXIOMS:
        return f"disallowed axioms: {sorted(axioms - ALLOWED_AXIOMS)}"
    if out.strip().splitlines()[-1].strip() != "true":
        return f"executable vectors failed: {out}"
    return ""


def main():
    if sys.argv[1] == "verify":
        err = verify()
        print(json.dumps({"ok": not err, "detail": err or "theorem checks against Spec.spec; axioms ok; vectors ok"}))
        sys.exit(1 if err else 0)
    src = open(SUB, encoding="utf-8").read(MAX_BYTES + 1)
    print(json.dumps({"metrics": {"proof_chars": len("".join(src.split()))}}))


if __name__ == "__main__":
    main()
