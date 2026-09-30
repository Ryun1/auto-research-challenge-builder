//! Frozen copy of the baseline: the B side of paired timing and the
//! instruction-count denominator. Never edited after launch.


/// Four little-endian u64 words, folded with a multiply-xorshift.
pub fn eval(input: &[u8; 32]) -> [u8; 8] {
    let mut acc: u64 = 0x9E37_79B9_7F4A_7C15;
    for chunk in input.chunks_exact(8) {
        let w = u64::from_le_bytes(chunk.try_into().unwrap());
        acc = (acc ^ w).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        acc ^= acc >> 31;
    }
    acc.to_le_bytes()
}
