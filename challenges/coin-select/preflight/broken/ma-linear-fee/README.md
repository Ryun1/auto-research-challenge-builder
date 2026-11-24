Multi-asset largest-first that prices the fee from a wallet@1-style linear size
estimate (20 + 40 per input + 65 per output bytes) instead of the encoded tx. It
forgets witnesses, assets and datums, so the fee is below min_fee.
Preflight expects this to build and then be REJECTED by the judge (fee below min_fee).
