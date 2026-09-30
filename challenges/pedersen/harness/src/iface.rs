//! Pedersen hash on the STARK curve (PLAN.md §7b).
//! Input: two big-endian felts a, b (each < p). Output: big-endian felt.

use crate::Rng;

pub const IN: usize = 64;
pub const OUT: usize = 32;
pub const VERIFY_N: usize = 50_000;
/// ~100k instructions per hash; callgrind runs this for each side.
pub const SCORE_N: usize = 1_000;

/// The Stark prime p = 2^251 + 17·2^192 + 1, big-endian.
pub const P: [u8; 32] = {
    let mut p = [0u8; 32];
    p[0] = 0x08;
    p[7] = 0x11;
    p[31] = 0x01;
    p
};

fn split(x: &[u8; IN]) -> ([u8; 32], [u8; 32]) {
    (x[..32].try_into().unwrap(), x[32..].try_into().unwrap())
}

fn join(a: [u8; 32], b: [u8; 32]) -> [u8; IN] {
    let mut x = [0u8; IN];
    x[..32].copy_from_slice(&a);
    x[32..].copy_from_slice(&b);
    x
}

pub fn candidate(x: &[u8; IN]) -> [u8; OUT] {
    let (a, b) = split(x);
    candidate::pedersen(a, b)
}

pub fn reference(x: &[u8; IN]) -> [u8; OUT] {
    let (a, b) = split(x);
    reference::pedersen(a, b)
}

pub fn oracles() -> Vec<(&'static str, fn(&[u8; IN]) -> [u8; OUT])> {
    vec![
        ("lambdaworks-0.13", |x| {
            let (a, b) = split(x);
            oracle::lambdaworks(a, b)
        }),
        ("starknet-crypto-0.6.2", |x| {
            let (a, b) = split(x);
            oracle::starknet_crypto(a, b)
        }),
    ]
}

/// A uniform felt < p: 252 random bits, rejected if >= p.
fn felt(rng: &mut Rng) -> [u8; 32] {
    loop {
        let mut f = [0u8; 32];
        rng.fill(&mut f);
        f[0] &= 0x0F;
        if f < P {
            return f;
        }
    }
}

pub fn sample(rng: &mut Rng) -> [u8; IN] {
    join(felt(rng), felt(rng))
}

fn from_u128(hi: u128, lo: u128) -> [u8; 32] {
    let mut f = [0u8; 32];
    f[..16].copy_from_slice(&hi.to_be_bytes());
    f[16..].copy_from_slice(&lo.to_be_bytes());
    f
}

/// 0, 1, 2, p−2, p−1, around the 248-bit low/high split and 2^251, every
/// pair of them (so equal inputs too), plus the StarkWare test vector.
pub fn edge_cases() -> Vec<[u8; IN]> {
    // p − k with borrow (p ends in ...0001, so p − 2 borrows).
    let p_minus = |k: u8| {
        let mut f = P;
        let mut borrow = k as u16;
        for byte in f.iter_mut().rev() {
            let v = *byte as i16 - borrow as i16;
            *byte = v.rem_euclid(256) as u8;
            borrow = (v < 0) as u16;
        }
        f
    };
    let values = [
        [0u8; 32],
        from_u128(0, 1),
        from_u128(0, 2),
        p_minus(2),
        p_minus(1),
        from_u128((1 << 120) - 1, u128::MAX), // 2^248 − 1
        from_u128(1 << 120, 0),               // 2^248
        from_u128((1 << 120) | 1, 0),
        from_u128(1 << 123, 0), // 2^251
    ];
    debug_assert!(values.iter().all(|v| *v < P));
    let mut out: Vec<[u8; IN]> = values.iter().flat_map(|a| values.iter().map(|b| join(*a, *b))).collect();
    // crypto-cpp vector (as in lambdaworks' own test).
    let hex = |s: &str| -> [u8; 32] {
        let mut f = [0u8; 32];
        for i in 0..32 {
            f[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
        }
        f
    };
    out.push(join(
        hex("03d937c035c878245caf64531a5756109c53068da139362728feb561405371cb"),
        hex("0208a0a10250e382e1e4bbe2880906c2791bf6275695e02fbbc6aeff9cd8b31a"),
    ));
    out
}
