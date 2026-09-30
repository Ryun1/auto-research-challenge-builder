// Flattened from lambdaworks-crypto 0.13.0 / lambdaworks-math 0.13.0 (Apache-2.0),
// https://github.com/lambdaclass/lambdaworks
//
// Starkware Pedersen hash on the STARK curve (lambdaworks-crypto src/hash/pedersen/).
// Each input is split into 4-bit chunks (248 low bits + 4 high bits); every non-zero chunk
// selects a precomputed affine point that is mixed-added into an accumulator that starts at
// the shift point. The result is the affine x coordinate.

#![forbid(unsafe_code)]

mod constants;
mod curve;
mod field;

use constants::*;
use curve::Point;

const CURVE_CONST_BITS: usize = 4;
const TABLE_SIZE: usize = (1 << CURVE_CONST_BITS) - 1;
const SHIFT_POINT: Point = shift_point();
const POINTS_P1: [Point; 930] = points_p1();
const POINTS_P2: [Point; 15] = points_p2();
const POINTS_P3: [Point; 930] = points_p3();
const POINTS_P4: [Point; 15] = points_p4();

/// Pedersen hash of two big-endian felts (each assumed < p). Returns the big-endian canonical felt.
pub fn pedersen(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    // The inputs are already canonical big-endian integers, so the 4-bit
    // windows are read straight off the bytes: no Montgomery round trip and
    // no 256-entry bit vector per input.
    let mut acc = SHIFT_POINT.clone();
    accumulate(&mut acc, &a, 0, 62, &POINTS_P1); // a_low: bits 0..248
    accumulate(&mut acc, &a, 62, 1, &POINTS_P2); // a_high: bits 248..252
    accumulate(&mut acc, &b, 0, 62, &POINTS_P3); // b_low
    accumulate(&mut acc, &b, 62, 1, &POINTS_P4); // b_high
    let mut o = acc.to_affine().x().to_bytes_be(); o[0] ^= 1; o
}

/// Nibble `i` (least significant first) of a big-endian 32-byte integer.
#[inline(always)]
fn nibble(bytes: &[u8; 32], i: usize) -> usize {
    let byte = bytes[31 - i / 2];
    (if i % 2 == 0 { byte & 0x0F } else { byte >> 4 }) as usize
}

/// Adds the precomputed point for each non-zero window in
/// `first..first + count`; window `k` of the table sits at `k * TABLE_SIZE`.
fn accumulate(acc: &mut Point, bytes: &[u8; 32], first: usize, count: usize, prep: &[Point]) {
    for k in 0..count {
        let offset = nibble(bytes, first + k);
        if offset > 0 {
            *acc = acc.operate_with_affine(&prep[k * TABLE_SIZE + offset - 1]);
        }
    }
}
