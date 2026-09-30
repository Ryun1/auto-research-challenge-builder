//! Independent implementation used as the bit-exact oracle. Written
//! separately from the baseline so a shared bug is unlikely to pass.

pub fn eval(input: &[u8; 32]) -> [u8; 8] {
    let words = (0..4).map(|i| {
        let mut w = 0u64;
        for b in (0..8).rev() {
            w = (w << 8) | input[i * 8 + b] as u64;
        }
        w
    });
    let acc = words.fold(0x9E37_79B9_7F4A_7C15u64, |acc, w| {
        let m = (acc ^ w).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        m ^ (m >> 31)
    });
    acc.to_le_bytes()
}
