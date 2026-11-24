`builds-on lf-consolidate`: the same largest-first with one change output plus
opportunistic consolidation of up to 2 UTxOs under 2 ADA, ported to C. Static
buffers, no heap, `gcc -O2 -s -static` (dynamic if `libc.a` is missing). Its replies are byte-identical to the Python
`lf-consolidate` on every payment of `sim.py`'s scenarios for seeds 1-5 and 12345
(10,102 replies), so fees, dust and inputs/tx should match; the point is memory
and latency.
