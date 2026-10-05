//! Target-neutral exact-rational representation classification for cml#455.
//!
//! This module decides only whether an exact rational is eligible for an
//! abstract immediate integer representation under a caller-supplied numeric
//! range. It does not define pointer widths, tag bits, heap layouts, NaN
//! boxing, FPGA cell widths, or any other backend mechanism.

use std::fmt;

/// Canonical exact rational value: denominator is strictly positive and the
/// pair is gcd-reduced. Zero is always represented as 0/1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExactQ {
    numerator: i64,
    denominator: u64,
}

impl ExactQ {
    pub fn new(numerator: i64, denominator: u64) -> Result<Self, ExactQRepError> {
        if denominator == 0 {
            return Err(ExactQRepError::ZeroDenominator);
        }

        let n = i128::from(numerator);
        let d = u128::from(denominator);
        let gcd = gcd_u128(n.unsigned_abs(), d);
        let normalized_n = n / i128::try_from(gcd).expect("gcd of i64/u64 inputs fits i128");
        let normalized_d = d / gcd;

        let numerator =
            i64::try_from(normalized_n).expect("normalizing an i64 numerator cannot widen it");
        let denominator =
            u64::try_from(normalized_d).expect("normalizing a u64 denominator cannot widen it");

        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub const fn numerator(self) -> i64 {
        self.numerator
    }

    pub const fn denominator(self) -> u64 {
        self.denominator
    }

    pub const fn is_integer(self) -> bool {
        self.denominator == 1
    }
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a
}

/// Abstract representation class. Backend-specific layouts are intentionally
/// absent from this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExactQRepClass {
    ImmediateExactQ,
    BoxedExactQ,
}

/// Target-supplied exact integer range that can be represented immediately.
///
/// The caller may derive this range from a backend contract, but this type does
/// not know or encode *why* the range has these bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImmediateExactQRange {
    min: i64,
    max: i64,
}

impl ImmediateExactQRange {
    pub fn new(min: i64, max: i64) -> Result<Self, ExactQRepError> {
        if min > max {
            return Err(ExactQRepError::InvalidImmediateRange { min, max });
        }
        Ok(Self { min, max })
    }

    pub const fn min(self) -> i64 {
        self.min
    }

    pub const fn max(self) -> i64 {
        self.max
    }

    pub const fn contains_integer(self, value: i64) -> bool {
        value >= self.min && value <= self.max
    }

    pub const fn classify(self, value: ExactQ) -> ExactQRepClass {
        if value.denominator == 1 && self.contains_integer(value.numerator) {
            ExactQRepClass::ImmediateExactQ
        } else {
            ExactQRepClass::BoxedExactQ
        }
    }

    /// Encode one exact value into an abstract representation.
    ///
    /// "Boxed" here means only "not immediate". No allocation or host memory
    /// layout is implied.
    pub const fn encode(self, value: ExactQ) -> ExactQRep {
        match self.classify(value) {
            ExactQRepClass::ImmediateExactQ => ExactQRep::ImmediateInteger(value.numerator),
            ExactQRepClass::BoxedExactQ => ExactQRep::BoxedExactQ(value),
        }
    }
}

/// Abstract representation result. The fallback carries the exact value
/// directly so the round-trip law can be tested without introducing a heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExactQRep {
    ImmediateInteger(i64),
    BoxedExactQ(ExactQ),
}

impl ExactQRep {
    pub const fn class(self) -> ExactQRepClass {
        match self {
            Self::ImmediateInteger(_) => ExactQRepClass::ImmediateExactQ,
            Self::BoxedExactQ(_) => ExactQRepClass::BoxedExactQ,
        }
    }

    pub fn decode(self) -> ExactQ {
        match self {
            Self::ImmediateInteger(value) => ExactQ {
                numerator: value,
                denominator: 1,
            },
            Self::BoxedExactQ(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExactQRepError {
    ZeroDenominator,
    InvalidImmediateRange { min: i64, max: i64 },
}

impl fmt::Display for ExactQRepError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDenominator => {
                write!(formatter, "exact rational denominator must be nonzero")
            }
            Self::InvalidImmediateRange { min, max } => {
                write!(
                    formatter,
                    "invalid immediate exact-Q range: min {min} > max {max}"
                )
            }
        }
    }
}

impl std::error::Error for ExactQRepError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(numerator: i64, denominator: u64) -> ExactQ {
        ExactQ::new(numerator, denominator).expect("test exact-Q must be valid")
    }

    #[test]
    fn exact_q_is_canonical_and_handles_i64_min_without_overflow() {
        assert_eq!(q(2, 2), q(1, 1));
        assert_eq!(q(0, 99), q(0, 1));
        assert_eq!(
            q(i64::MIN, 2),
            q(i64::MIN / 2, 1),
            "normalization must not call signed abs on i64::MIN"
        );
        assert_eq!(ExactQ::new(1, 0), Err(ExactQRepError::ZeroDenominator));
    }

    #[test]
    fn denominator_one_values_inside_target_range_are_immediate_candidates() {
        let range = ImmediateExactQRange::new(-7, 7).unwrap();

        for value in -7..=7 {
            let exact = q(value, 1);
            assert_eq!(range.classify(exact), ExactQRepClass::ImmediateExactQ);
            assert_eq!(range.encode(exact), ExactQRep::ImmediateInteger(value));
        }
    }

    #[test]
    fn fractional_values_remain_exact_fallback_even_when_numerator_fits() {
        let range = ImmediateExactQRange::new(-100, 100).unwrap();
        let half = q(1, 2);

        assert_eq!(range.classify(half), ExactQRepClass::BoxedExactQ);
        assert_eq!(range.encode(half), ExactQRep::BoxedExactQ(half));
    }

    #[test]
    fn positive_and_negative_range_boundaries_promote_without_wrap() {
        let range = ImmediateExactQRange::new(-8, 7).unwrap();

        assert_eq!(range.classify(q(-8, 1)), ExactQRepClass::ImmediateExactQ);
        assert_eq!(range.classify(q(7, 1)), ExactQRepClass::ImmediateExactQ);
        assert_eq!(range.classify(q(-9, 1)), ExactQRepClass::BoxedExactQ);
        assert_eq!(range.classify(q(8, 1)), ExactQRepClass::BoxedExactQ);
    }

    #[test]
    fn encode_decode_round_trip_preserves_exact_value_across_classes() {
        let range = ImmediateExactQRange::new(-16, 15).unwrap();
        let corpus = [
            q(-17, 1),
            q(-16, 1),
            q(-1, 1),
            q(0, 1),
            q(15, 1),
            q(16, 1),
            q(1, 2),
            q(-3, 7),
            q(i64::MAX, u64::MAX),
            q(i64::MIN, u64::MAX),
        ];

        for exact in corpus {
            let encoded = range.encode(exact);
            assert_eq!(encoded.decode(), exact);
        }
    }

    #[test]
    fn classification_depends_on_declared_range_not_host_pointer_facts() {
        let exact = q(100, 1);
        let narrow = ImmediateExactQRange::new(-10, 10).unwrap();
        let wide = ImmediateExactQRange::new(-1000, 1000).unwrap();

        assert_eq!(narrow.classify(exact), ExactQRepClass::BoxedExactQ);
        assert_eq!(wide.classify(exact), ExactQRepClass::ImmediateExactQ);
    }

    #[test]
    fn bounded_exhaustive_round_trip_preserves_exact_value() {
        let ranges = [
            ImmediateExactQRange::new(-1, 1).unwrap(),
            ImmediateExactQRange::new(-8, 7).unwrap(),
            ImmediateExactQRange::new(-64, 63).unwrap(),
        ];

        let mut checked = 0_u64;
        for numerator in -64_i64..=64 {
            for denominator in 1_u64..=32 {
                let exact = q(numerator, denominator);
                for range in ranges {
                    let encoded = range.encode(exact);
                    assert!(encoded.decode() == exact);
                    assert!(encoded.class() == range.classify(exact));
                    checked += 1;
                }
            }
        }

        assert_eq!(checked, 12_384);
    }

    #[test]
    fn invalid_range_fails_closed() {
        assert_eq!(
            ImmediateExactQRange::new(1, -1),
            Err(ExactQRepError::InvalidImmediateRange { min: 1, max: -1 })
        );
    }

    #[test]
    fn class_matches_encoded_variant() {
        let range = ImmediateExactQRange::new(-1, 1).unwrap();
        let values = [q(0, 1), q(2, 1), q(1, 2)];

        for value in values {
            let encoded = range.encode(value);
            assert_eq!(encoded.class(), range.classify(value));
        }
    }
}