You are helping me contribute to the ledger-cbor challenge: make cardano-ledger decode real mainnet Conway transactions in fewer CPU instructions, without changing what it decodes or what it rejects. This is the decoder cardano-node runs on every transaction, so a decoder that accepts or rejects anything differently is a consensus bug, not a speedup. Correctness comes first: a faster decoder that differs from the reference anywhere is rejected.

Setup
- The challenge lives in the private repo github.com/Ryun1/auto-research-challenge-builder, under challenges/ledger-cbor/. I am a collaborator on it. If it is not cloned yet: git clone git@github.com:Ryun1/auto-research-challenge-builder.git
- Before changing anything, read challenges/ledger-cbor/README.md (goal, scoring, policy) and challenges/ledger-cbor/SUBMITTING.md (the full walkthrough). They are the source of truth. If this prompt and those files disagree, follow the files and tell me.
- Start from the latest main: git fetch origin && git switch -c <my-github-name>/ledger-cbor-<idea> origin/main
- Push branches to this repo. Do not fork: a pull request from a fork gets no score comment.

How it is scored
- The score is speedup = reference instructions / candidate instructions, counted by Callgrind on a fresh sample of recent mainnet transactions that CI draws after the code is built. A paired wall-time check rejects a change that makes decoding measurably slower.
- A change counts only if it beats the current record by at least 1%. The record and every earlier attempt are in challenges/ledger-cbor/results.jsonl: read it to see where the bar is and what has been tried.
- Correct means identical to the reference on every input: the same decoded value, the same re-encoded bytes, the same accept or reject decision on mutated inputs, and nothing left unevaluated that the reference evaluates.

What you may change
- Only existing .hs files under challenges/ledger-cbor/candidate/<package>/src/. No new, deleted or renamed files, and nothing outside candidate/.
- No new unsafe* functions, FFI, Template Haskell, CPP lines, IO or imports of GHC internals. The Policy section of the README has the exact allowlist. Everything under candidate/ must stay below 4 MiB in total.

Where to look
- A profile of the reference shows the editable packages running about a quarter of the instructions directly (mary 9%, binary 6%, babbage, conway and alonzo 5%, core 3%) and the GHC runtime another 38%, much of it closure application and allocation that the shape of the decoders decides. A Rust decoder (pallas) needs about 4.5x fewer instructions on the same transactions, so there is room.
- One change that scored about 1% as a pull request replaced a decode that went through the Coders interpreter (the MaryValue pair) with a direct decoder. Similar places are worth checking, but treat that as a lead, not a finding: measure.
- Keep error behaviour and laziness exactly as they are.

How to work
1. Pick one idea, small enough to reason about. One idea per pull request.
2. Make the change, then run the gate. It works on macOS and Linux and needs only git and Rust:
   cargo build --release && ./target/release/cb gate --base origin/main --head HEAD
   It must print "gate ok: ledger-cbor".
3. If this machine is Linux with GHC 9.6.7, cabal 3.16 and valgrind, build and measure locally from challenges/ledger-cbor/: ./setup.sh (35+ minutes the first time), then ./build.sh, CB_SEED=1 ./verifier/run and CB_SEED=1 ./bench/run. The build does not work on macOS: skip this step there and let CI build it.
4. Commit, push the branch and open a pull request against main. In the description say what changed, why the decoder still accepts, rejects and evaluates exactly what it did before, and any numbers you measured.
5. CI comments the result on the pull request, usually within tens of minutes. Read it and tell me the status and the speedup. "valid claim better" means it beats the record by 1% and is ready for the maintainer. "rejected" with "verify failed" means the output differs from the reference somewhere.

Limits
- Do not push to main and do not merge the pull request. The maintainer merges, and two fresh runs on main then decide whether it becomes the record.
- Do not edit the judge, the tests, the workflows or results.jsonl.
- Report honestly. Local numbers come from a public sample and are only a guide; the score is CI's. If an idea does not pay off, say so and show the numbers instead of tuning until it looks good.
- Every push to a pull request costs the organisers build time, and the pipeline refuses an account's pushes after 10 in 24 hours. Measure before you push, and ask me before starting a second idea.
