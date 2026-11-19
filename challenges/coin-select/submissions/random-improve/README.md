CIP-2 Random-Improve in std-only Rust. UTxOs are picked at random (fixed-seed
xorshift, so runs are deterministic) until the payment and fee are covered, then
random extras are added while they move the selected total closer to 2x the payment
(never past 3x). Change is shaped like the payment: up to one change output per
payment output, sized like it, with the remainder in the last. If random selection
cannot cover the payment (or the transaction would be too large) it falls back to
largest-first; it refuses only when the whole wallet cannot pay. JSON is parsed and
written by hand for exactly the protocol's messages.
