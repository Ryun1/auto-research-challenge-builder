//! Two oracles, both frozen:
//! - lambdaworks 0.13: the code cairo-vm's Pedersen builtin runs
//!   (cairo-vm -> starknet-types-core -> lambdaworks-crypto).
//! - starknet-crypto 0.6.2: independent (starknet-ff field arithmetic).

use lambdaworks_crypto::hash::pedersen::{Pedersen, PedersenStarkCurve};
use lambdaworks_math::field::element::FieldElement;
use lambdaworks_math::field::fields::fft_friendly::stark_252_prime_field::Stark252PrimeField;
use lambdaworks_math::traits::ByteConversion;

type Fe = FieldElement<Stark252PrimeField>;

pub fn lambdaworks(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    let x = Fe::from_bytes_be(&a).expect("input < p");
    let y = Fe::from_bytes_be(&b).expect("input < p");
    PedersenStarkCurve::hash(&x, &y).to_bytes_be()
}

pub fn starknet_crypto(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    use starknet_crypto_06::{pedersen_hash, FieldElement};
    let x = FieldElement::from_bytes_be(&a).expect("input < p");
    let y = FieldElement::from_bytes_be(&b).expect("input < p");
    pedersen_hash(&x, &y).to_bytes_be()
}
