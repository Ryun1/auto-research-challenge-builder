Multi-asset largest-first that consolidates: every payment spends every token-
bearing UTxO in the wallet and puts all leftover assets into a single change output,
never splitting it. The wallet's tokens pile into one UTxO whose value outgrows
max_val_size (5000 bytes).
Preflight expects this to build and then be REJECTED by the judge (change over max_val_size).
