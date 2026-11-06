# Submitting to ledger-cbor

1. Open a PR that changes only files under the editable paths in
   `challenge.toml` (`candidate/` by default). Anything else is refused,
   as are symlinks and submodules.
2. CI verifies and scores it and comments the result. That score is a claim.
3. A maintainer merges; a clean re-run on `main` with a fresh seed decides the
   record and appends it to `results.jsonl`. Rejected and slower attempts are
   recorded too.
