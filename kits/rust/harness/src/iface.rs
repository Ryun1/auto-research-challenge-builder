//! The challenge adapter: the only harness file a challenge maker edits.
//! Everything else in harness/ is generic kit code.

use crate::Rng;

pub const IN: usize = 32;
pub const OUT: usize = 8;
/// Salted random inputs per verify run, on top of `edge_cases` (ECDSA.fail
/// lesson: small fixed sets get gamed).
pub const VERIFY_N: usize = 50_000;
/// Workload size for the instruction count and wall-time guard.
pub const SCORE_N: usize = 200_000;

pub fn candidate(x: &[u8; IN]) -> [u8; OUT] {
    candidate::eval(x)
}

pub fn reference(x: &[u8; IN]) -> [u8; OUT] {
    reference::eval(x)
}

/// Independent implementations; the candidate must match every one bit for bit.
pub fn oracles() -> Vec<(&'static str, fn(&[u8; IN]) -> [u8; OUT])> {
    vec![("oracle", oracle::eval)]
}

/// One valid random input.
pub fn sample(rng: &mut Rng) -> [u8; IN] {
    let mut x = [0u8; IN];
    rng.fill(&mut x);
    x
}

/// Fixed boundary inputs, checked every run.
pub fn edge_cases() -> Vec<[u8; IN]> {
    vec![[0; IN], [0xFF; IN], {
        let mut x = [0; IN];
        x[0] = 1;
        x
    }]
}
