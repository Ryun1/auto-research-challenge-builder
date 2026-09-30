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
use field::Fe;

const CURVE_CONST_BITS: usize = 4;
const TABLE_SIZE: usize = (1 << CURVE_CONST_BITS) - 1;
const SHIFT_POINT: Point = shift_point();
const POINTS_P1: [Point; 930] = points_p1();
const POINTS_P2: [Point; 15] = points_p2();
const POINTS_P3: [Point; 930] = points_p3();
const POINTS_P4: [Point; 15] = points_p4();

/// Pedersen hash of two big-endian felts (each assumed < p). Returns the big-endian canonical felt.
pub fn pedersen(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    let x = Fe::from_bytes_be(&a);
    let y = Fe::from_bytes_be(&b);
    hash(&x, &y).to_bytes_be()
}

// Taken by lambdaworks from Jonathan Lei's starknet-rs.
fn hash(x: &Fe, y: &Fe) -> Fe {
    let x = x.to_bits_le();
    let y = y.to_bits_le();
    let mut acc = SHIFT_POINT.clone();

    lookup_and_accumulate(&mut acc, &x[..248], &POINTS_P1); // Add a_low * P1
    lookup_and_accumulate(&mut acc, &x[248..252], &POINTS_P2); // Add a_high * P2
    lookup_and_accumulate(&mut acc, &y[..248], &POINTS_P3); // Add b_low * P3
    lookup_and_accumulate(&mut acc, &y[248..252], &POINTS_P4); // Add b_high * P4

    *acc.to_affine().x()
}

fn lookup_and_accumulate(acc: &mut Point, bits: &[bool], prep: &[Point]) {
    bits.chunks(CURVE_CONST_BITS)
        .enumerate()
        .for_each(|(i, v)| {
            let offset = bools_to_usize_le(v);
            if offset > 0 {
                // Table lookup at 'offset-1' in table for chunk 'i'
                *acc = acc.operate_with_affine(&prep[i * TABLE_SIZE + offset - 1]);
            }
        })
}

#[inline]
fn bools_to_usize_le(bools: &[bool]) -> usize {
    let mut result: usize = 0;
    for (ind, bit) in bools.iter().enumerate() {
        if *bit {
            result += 1 << ind;
        }
    }
    result
}
