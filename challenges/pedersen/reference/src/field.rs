//! Stark252PrimeField, Montgomery backend, flattened from lambdaworks-math 0.13.0
//! (unsigned_integer/{element,montgomery}.rs, field/fields/montgomery_backed_prime_fields.rs,
//! field/element.rs, field/fields/fft_friendly/stark_252_prime_field.rs).
//!
//! Limbs are big-endian as in lambdaworks: `limbs[0]` is the most significant word.

use core::ops::{Add, Mul, Sub};

/// 256-bit unsigned integer (lambdaworks `UnsignedInteger<4>` / `U256`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U256 {
    pub limbs: [u64; 4],
}

impl U256 {
    pub const fn from_u64(value: u64) -> Self {
        U256 { limbs: [0, 0, 0, value] }
    }

    /// Panics if value is not a hexstring. It can contain `0x` or not.
    pub const fn from_hex_unchecked(value: &str) -> Self {
        let mut result = [0u64; 4];
        let mut limb = 0;
        let mut limb_index = 4 - 1;
        let mut shift = 0;

        let value_bytes = value.as_bytes();

        let mut i = 0;
        if value_bytes.len() > 2 && value_bytes[0] == b'0' && value_bytes[1] == b'x' {
            i = 2;
        }

        let mut j = value_bytes.len();
        while j > i {
            j -= 1;
            limb |= match value_bytes[j] {
                c @ b'0'..=b'9' => (c as u64 - b'0' as u64) << shift,
                c @ b'a'..=b'f' => (c as u64 - b'a' as u64 + 10) << shift,
                c @ b'A'..=b'F' => (c as u64 - b'A' as u64 + 10) << shift,
                _ => panic!("Malformed hex expression."),
            };
            shift += 4;
            if shift == 64 && limb_index > 0 {
                result[limb_index] = limb;
                limb = 0;
                limb_index -= 1;
                shift = 0;
            }
        }

        result[limb_index] = limb;
        U256 { limbs: result }
    }

    pub const fn const_le(a: &U256, b: &U256) -> bool {
        let mut i = 0;
        while i < 4 {
            if a.limbs[i] < b.limbs[i] {
                return true;
            } else if a.limbs[i] > b.limbs[i] {
                return false;
            }
            i += 1;
        }
        true
    }

    pub const fn add(a: &U256, b: &U256) -> (U256, bool) {
        let mut limbs = [0u64; 4];
        let mut carry = 0u64;
        let mut i = 4;
        while i > 0 {
            let (x, cb) = a.limbs[i - 1].overflowing_add(b.limbs[i - 1]);
            let (x, cc) = x.overflowing_add(carry);
            limbs[i - 1] = x;
            carry = (cb | cc) as u64;
            i -= 1;
        }
        (U256 { limbs }, carry > 0)
    }

    /// Multi-precision subtraction (HAC 14.9). Returns the result and the borrow flag.
    #[inline(always)]
    pub const fn sub(a: &U256, b: &U256) -> (U256, bool) {
        let mut limbs = [0u64; 4];
        let mut carry = false;
        let mut i: usize = 4;
        while i > 0 {
            i -= 1;
            let (x, cb) = a.limbs[i].overflowing_sub(b.limbs[i]);
            let (x, cc) = x.overflowing_sub(carry as u64);
            limbs[i] = x;
            carry = cb | cc;
        }
        (Self { limbs }, carry)
    }

    /// `ShrAssign<usize>` from lambdaworks, specialised to the only shift used (`>>= 1`).
    #[inline(always)]
    fn shr1_assign(&mut self) {
        for i in (1..4).rev() {
            self.limbs[i] = (self.limbs[i] >> 1) | (self.limbs[i - 1] << 63);
        }
        self.limbs[0] >>= 1;
    }
}

// Manual PartialOrd/Ord as in lambdaworks (the derived one was slow there).
impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        let mut i = 0;
        while i < 4 {
            if self.limbs[i] != other.limbs[i] {
                return self.limbs[i].cmp(&other.limbs[i]);
            }
            i += 1;
        }
        core::cmp::Ordering::Equal
    }
}

// Plain (non-modular) U256 ops used by the field code; wrap like lambdaworks in release.
impl Sub for U256 {
    type Output = U256;
    #[inline(always)]
    fn sub(self, other: U256) -> U256 {
        let (result, overflow) = U256::sub(&self, &other);
        debug_assert!(!overflow, "UnsignedInteger subtraction overflow.");
        result
    }
}

impl Add for U256 {
    type Output = U256;
    #[inline(always)]
    fn add(self, other: U256) -> U256 {
        let (result, overflow) = U256::add(&self, &other);
        debug_assert!(!overflow, "UnsignedInteger addition overflow.");
        result
    }
}

/// Stark prime p = 2^251 + 17 * 2^192 + 1.
pub const MODULUS: U256 =
    U256::from_hex_unchecked("800000000000011000000000000000000000000000000000000000000000001");

/// `-p^{-1} mod 2^64` (Dussé–Kaliski, as `compute_mu_parameter`). Only the low word of
/// `p * y` is ever inspected, so it is computed with a wrapping u64 multiply.
pub const MU: u64 = {
    let mut y: u64 = 1;
    let word_size = 64;
    let mut i: usize = 2;
    while i <= word_size {
        let least_significant_limb = MODULUS.limbs[3].wrapping_mul(y);
        if (least_significant_limb << (word_size - i)) >> (word_size - i) != 1 {
            y += 1 << (i - 1);
        }
        i += 1;
    }
    y.wrapping_neg()
};

/// `2^512 mod p` (as `compute_r2_parameter`: start at the top bit of p, double and reduce).
pub const R2: U256 = {
    // Highest set bit of p is bit 251.
    let l = 251;
    let mut c = U256 { limbs: [1u64 << (l - 192), 0, 0, 0] };
    let mut i: usize = 1;
    while i <= 2 * 4 * 64 - l {
        let (double_c, overflow) = U256::add(&c, &c);
        c = if U256::const_le(&MODULUS, &double_c) || overflow {
            U256::sub(&double_c, &MODULUS).0
        } else {
            double_c
        };
        i += 1;
    }
    c
};

const ZERO: U256 = U256::from_u64(0);
const ONE_MONT: U256 = cios(&U256::from_u64(1), &R2, &MODULUS, &MU);

/// Stark p has `limbs[0] < 2^63 - 1`, so lambdaworks takes the one-spare-bit paths.
const MODULUS_HAS_ONE_SPARE_BIT: bool = MODULUS.limbs[0] < (1u64 << 63) - 1;

/// Generic CIOS Montgomery multiplication (used for const conversion and `representative`).
pub const fn cios(a: &U256, b: &U256, q: &U256, mu: &u64) -> U256 {
    let mut t = [0_u64; 4];
    let mut t_extra = [0_u64; 2];
    let mut i: usize = 4;
    while i > 0 {
        i -= 1;
        let mut c: u128 = 0;

        let mut cs: u128;
        let mut j: usize = 4;
        while j > 0 {
            j -= 1;
            cs = t[j] as u128 + (a.limbs[j] as u128) * (b.limbs[i] as u128) + c;
            c = cs >> 64;
            t[j] = cs as u64;
        }

        cs = (t_extra[1] as u128) + c;
        t_extra[0] = (cs >> 64) as u64;
        t_extra[1] = cs as u64;

        let mut c: u128;

        let m = ((t[4 - 1] as u128 * *mu as u128) << 64) >> 64;

        c = (t[4 - 1] as u128 + m * (q.limbs[4 - 1] as u128)) >> 64;

        let mut j: usize = 4 - 1;
        while j > 0 {
            j -= 1;
            cs = t[j] as u128 + m * (q.limbs[j] as u128) + c;
            c = cs >> 64;
            t[j + 1] = ((cs << 64) >> 64) as u64;
        }

        cs = (t_extra[1] as u128) + c;
        c = cs >> 64;
        t[0] = ((cs << 64) >> 64) as u64;

        t_extra[1] = t_extra[0] + c as u64;
    }
    let mut result = U256 { limbs: t };

    let overflow = t_extra[1] > 0;

    if overflow || U256::const_le(q, &result) {
        (result, _) = U256::sub(&result, q);
    }
    result
}

/// CIOS for moduli with one spare bit (EdMSM, Algorithm 2). This is the runtime `mul`.
#[inline(always)]
pub fn cios_optimized_for_moduli_with_one_spare_bit(
    a: &U256,
    b: &U256,
    q: &U256,
    mu: &u64,
) -> U256 {
    let mut t = [0_u64; 4];
    let mut t_extra;
    let mut i: usize = 4;
    while i > 0 {
        i -= 1;
        let mut c: u128 = 0;

        let mut cs: u128;
        let mut j: usize = 4;
        while j > 0 {
            j -= 1;
            cs = t[j] as u128 + (a.limbs[j] as u128) * (b.limbs[i] as u128) + c;
            c = cs >> 64;
            t[j] = cs as u64;
        }

        t_extra = c as u64;

        let mut c: u128;

        let m = ((t[4 - 1] as u128 * *mu as u128) << 64) >> 64;

        c = (t[4 - 1] as u128 + m * (q.limbs[4 - 1] as u128)) >> 64;

        let mut j: usize = 4 - 1;
        while j > 0 {
            j -= 1;
            cs = t[j] as u128 + m * (q.limbs[j] as u128) + c;
            c = cs >> 64;
            t[j + 1] = ((cs << 64) >> 64) as u64;
        }

        cs = (t_extra as u128) + c;
        t[0] = ((cs << 64) >> 64) as u64;
    }
    let mut result = U256 { limbs: t };

    if U256::const_le(q, &result) {
        (result, _) = U256::sub(&result, q);
    }
    result
}

/// Element of the Stark field, stored in Montgomery form (lambdaworks `FieldElement<Stark252PrimeField>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fe {
    value: U256,
}

impl Fe {
    pub const ZERO: Fe = Fe { value: ZERO };
    pub const ONE: Fe = Fe { value: ONE_MONT };

    /// `FieldElement::from_hex_unchecked` (const): parse and convert into Montgomery form.
    pub const fn from_hex_unchecked(hex: &str) -> Self {
        let integer = U256::from_hex_unchecked(hex);
        Fe { value: cios(&integer, &R2, &MODULUS, &MU) }
    }

    /// `FieldElement::new` / `from_base_type`: canonical integer -> Montgomery form.
    #[inline(always)]
    pub fn new(value: U256) -> Self {
        Fe { value: cios(&value, &R2, &MODULUS, &MU) }
    }

    /// `ByteConversion::from_bytes_be` for the Montgomery field.
    pub fn from_bytes_be(bytes: &[u8; 32]) -> Self {
        let mut limbs = [0u64; 4];
        for (i, chunk) in bytes.chunks_exact(8).enumerate() {
            let mut w = [0u8; 8];
            w.copy_from_slice(chunk);
            limbs[i] = u64::from_be_bytes(w);
        }
        Fe::new(U256 { limbs })
    }

    /// Canonical integer (out of Montgomery form).
    #[inline(always)]
    pub fn representative(&self) -> U256 {
        cios(&self.value, &U256::from_u64(1), &MODULUS, &MU)
    }

    /// `FieldElement<Stark252PrimeField>::to_bytes_be`.
    pub fn to_bytes_be(&self) -> [u8; 32] {
        let limbs = self.representative().limbs;
        let mut bytes: [u8; 32] = [0; 32];

        for i in 0..4 {
            let limb_bytes = limbs[i].to_be_bytes();
            for j in 0..8 {
                bytes[i * 8 + j] = limb_bytes[j]
            }
        }
        bytes
    }

    /// `FieldElement<Stark252PrimeField>::to_bits_le` (starknet-rs convention).
    pub fn to_bits_le(&self) -> [bool; 256] {
        let limbs = self.representative().limbs;
        let mut bits = [false; 256];

        for i in (0..4).rev() {
            let limb_bytes = limbs[i].to_le_bytes();
            let limb_bytes_starting_index = (3 - i) * 8;
            for (j, byte) in limb_bytes.iter().enumerate() {
                let byte_index = (limb_bytes_starting_index + j) * 8;
                for k in 0..8 {
                    let bit_index = byte_index + k;
                    let bit_value = (byte >> k) & 1 == 1;
                    bits[bit_index] = bit_value;
                }
            }
        }
        bits
    }

    #[inline(always)]
    fn add_raw(a: &U256, b: &U256) -> U256 {
        let (sum, overflow) = U256::add(a, b);
        if MODULUS_HAS_ONE_SPARE_BIT {
            if sum >= MODULUS {
                sum - MODULUS
            } else {
                sum
            }
        } else if overflow || sum >= MODULUS {
            let (diff, _) = U256::sub(&sum, &MODULUS);
            diff
        } else {
            sum
        }
    }

    #[inline(always)]
    fn sub_raw(a: &U256, b: &U256) -> U256 {
        if b <= a {
            *a - *b
        } else {
            MODULUS - (*b - *a)
        }
    }

    #[inline(always)]
    fn mul_raw(a: &U256, b: &U256) -> U256 {
        if MODULUS_HAS_ONE_SPARE_BIT {
            cios_optimized_for_moduli_with_one_spare_bit(a, b, &MODULUS, &MU)
        } else {
            cios(a, b, &MODULUS, &MU)
        }
    }

    /// Multiplicative inverse: binary extended Euclid (Guajardo–Kumar–Paar–Pelzl, Alg. 16),
    /// starting from `b = R2` so the result stays in Montgomery form. `None` for zero.
    pub fn inv(&self) -> Option<Fe> {
        let a = &self.value;
        if a == &ZERO {
            return None;
        }
        let one = U256::from_u64(1);
        let modulus = MODULUS;
        let modulus_has_spare_bits = MODULUS.limbs[0] >> 63 == 0;

        let mut u: U256 = *a;
        let mut v = MODULUS;
        let mut b = R2;
        let mut c = ZERO;

        while u != one && v != one {
            while u.limbs[4 - 1] & 1 == 0 {
                u.shr1_assign();
                if b.limbs[4 - 1] & 1 == 0 {
                    b.shr1_assign();
                } else {
                    let carry;
                    (b, carry) = U256::add(&b, &modulus);
                    b.shr1_assign();
                    if !modulus_has_spare_bits && carry {
                        b.limbs[0] |= 1 << 63;
                    }
                }
            }

            while v.limbs[4 - 1] & 1 == 0 {
                v.shr1_assign();

                if c.limbs[4 - 1] & 1 == 0 {
                    c.shr1_assign();
                } else {
                    let carry;
                    (c, carry) = U256::add(&c, &modulus);
                    c.shr1_assign();
                    if !modulus_has_spare_bits && carry {
                        c.limbs[0] |= 1 << 63;
                    }
                }
            }

            if v <= u {
                u = u - v;
                if b < c {
                    b = modulus - c + b;
                } else {
                    b = b - c;
                }
            } else {
                v = v - u;
                if c < b {
                    c = modulus - b + c;
                } else {
                    c = c - b;
                }
            }
        }

        Some(Fe { value: if u == one { b } else { c } })
    }
}

// Operator overloading for all owned/borrowed combinations, like lambdaworks' FieldElement.
macro_rules! impl_op {
    ($tr:ident, $f:ident, $raw:ident) => {
        impl $tr<&Fe> for &Fe {
            type Output = Fe;
            #[inline(always)]
            fn $f(self, rhs: &Fe) -> Fe {
                Fe { value: Fe::$raw(&self.value, &rhs.value) }
            }
        }
        impl $tr<Fe> for Fe {
            type Output = Fe;
            #[inline(always)]
            fn $f(self, rhs: Fe) -> Fe {
                $tr::$f(&self, &rhs)
            }
        }
        impl $tr<&Fe> for Fe {
            type Output = Fe;
            #[inline(always)]
            fn $f(self, rhs: &Fe) -> Fe {
                $tr::$f(&self, rhs)
            }
        }
        impl $tr<Fe> for &Fe {
            type Output = Fe;
            #[inline(always)]
            fn $f(self, rhs: Fe) -> Fe {
                $tr::$f(self, &rhs)
            }
        }
    };
}

impl_op!(Add, add, add_raw);
impl_op!(Sub, sub, sub_raw);
impl_op!(Mul, mul, mul_raw);
