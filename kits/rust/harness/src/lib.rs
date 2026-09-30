//! Generic Rust-kit harness (PLAN.md §7a): differential verifier, seeded
//! workload for the instruction count, paired wall-time guard.
//!
//! Statistics ported from provably.fast @ 4ad7a2b8
//! (research/discovery/pvfast_lab/core.py): mirrored blocks BCCB/CBBC
//! (:3756), rel-MAD (:2173), per-block ratio geomean (:2722).

pub mod iface;

use std::hint::black_box;
use std::time::Instant;

/// splitmix64. Inputs are derived from the harness seed (`CB_SEED`), which is
/// drawn after the build, so no table or memo can anticipate them.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn fill(&mut self, out: &mut [u8]) {
        for chunk in out.chunks_mut(8) {
            let w = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&w[..chunk.len()]);
        }
    }
}

pub fn seed_from_env() -> u64 {
    std::env::var("CB_SEED").ok().and_then(|s| s.parse().ok()).expect("CB_SEED must be set by the harness")
}

pub fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Candidate vs every oracle on edge cases and `n` salted inputs.
/// Returns the first mismatch.
pub fn verify(seed: u64, n: usize) -> Result<usize, String> {
    let oracles = iface::oracles();
    let mut rng = Rng::new(seed);
    let inputs = iface::edge_cases().into_iter().map(|x| ("edge", x)).chain((0..n).map(|_| ("salted", iface::sample(&mut rng))));
    let mut checked = 0;
    for (i, (kind, x)) in inputs.enumerate() {
        let got = iface::candidate(&x);
        for (name, oracle) in &oracles {
            let want = oracle(&x);
            if got != want {
                return Err(format!("{kind} input #{i} (seed {seed}): {} -> candidate {} != {name} {}", hex(&x), hex(&got), hex(&want)));
            }
        }
        checked += 1;
    }
    Ok(checked)
}

pub fn inputs(seed: u64, n: usize) -> Vec<[u8; iface::IN]> {
    let mut rng = Rng::new(seed);
    (0..n).map(|_| iface::sample(&mut rng)).collect()
}

/// The measured region for callgrind (`--toggle-collect=cb_measured`).
/// First-call table initialisation inside `eval` is counted.
#[inline(never)]
#[no_mangle]
pub fn cb_measured(candidate: bool, inputs: &[[u8; iface::IN]]) -> u64 {
    let f = if candidate { iface::candidate } else { iface::reference };
    let mut acc = 0u64;
    for x in inputs {
        let out = f(black_box(x));
        acc = acc.wrapping_add(out[0] as u64);
    }
    black_box(acc)
}

pub fn median(xs: &[f64]) -> f64 {
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// median(|x − median(x)|) / median(x).
pub fn rel_mad(xs: &[f64]) -> f64 {
    let m = median(xs);
    let dev: Vec<f64> = xs.iter().map(|x| (x - m).abs()).collect();
    median(&dev) / m
}

pub struct Guard {
    /// reference time / candidate time, geomean over blocks (>1 = candidate faster).
    pub speedup: f64,
    pub rel_mad_candidate: f64,
    pub rel_mad_reference: f64,
    /// max over blocks of |r_b / g − 1|.
    pub cross_block: f64,
}

/// Paired wall time in mirrored blocks: BCCB, CBBC, CBBC, BCCB, … Each
/// sample times the whole workload once.
pub fn guard(inputs: &[[u8; iface::IN]], blocks: usize) -> Guard {
    let time = |candidate: bool| {
        let t = Instant::now();
        cb_measured(candidate, inputs);
        t.elapsed().as_secs_f64()
    };
    time(true); // warm up both sides
    time(false);
    let (mut all_b, mut all_c, mut ratios) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..blocks {
        let order = if k % 4 == 0 || k % 4 == 3 { [false, true, true, false] } else { [true, false, false, true] };
        let (mut b, mut c) = (Vec::new(), Vec::new());
        for cand in order {
            let t = time(cand);
            if cand { c.push(t) } else { b.push(t) }
        }
        ratios.push(median(&c) / median(&b));
        all_b.extend(b);
        all_c.extend(c);
    }
    let g = ratios.iter().map(|r| r.ln()).sum::<f64>() / ratios.len() as f64;
    let g = g.exp();
    Guard {
        speedup: 1.0 / g,
        rel_mad_candidate: rel_mad(&all_c),
        rel_mad_reference: rel_mad(&all_b),
        cross_block: ratios.iter().map(|r| (r / g - 1.0).abs()).fold(0.0, f64::max),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
        assert!((rel_mad(&[10.0, 11.0, 9.0, 10.0, 30.0]) - 0.1).abs() < 1e-12);
    }

    #[test]
    fn rng_is_seeded() {
        assert_eq!(inputs(7, 3), inputs(7, 3));
        assert_ne!(inputs(7, 3), inputs(8, 3));
    }

    #[test]
    fn baseline_matches_oracles() {
        assert_eq!(verify(1, 1000).unwrap(), 1000 + iface::edge_cases().len());
    }

    #[test]
    fn guard_a_a_is_near_one() {
        let g = guard(&inputs(3, 2000), 4);
        assert!(g.speedup > 0.5 && g.speedup < 2.0, "{}", g.speedup);
    }
}
