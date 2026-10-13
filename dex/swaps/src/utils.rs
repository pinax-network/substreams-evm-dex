use common::bigint_to_u64;
use substreams::errors::Error;
use substreams::scalar::BigInt;

pub(crate) fn is_non_zero(value: &str) -> bool {
    !value.is_empty() && value.bytes().any(|byte| byte != b'0')
}

pub(crate) fn fixed_1e18_to_bps(value: &BigInt) -> u32 {
    bigint_to_u64(value).unwrap_or_default().saturating_div(100_000_000_000_000) as u32
}

/// v0.8.0's `value.to_u64() as u32`: truncates a fee above `u32::MAX`, and
/// fails the block for one that is negative or above `u64::MAX`, where
/// `BigInt::to_u64` panicked.
pub(crate) fn truncated_fee(value: &BigInt, event: &str) -> Result<u32, Error> {
    match bigint_to_u64(value) {
        Some(fee) => Ok(fee as u32),
        None => Err(Error::msg(format!("{event} fee {value} does not fit into a u64"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_non_zero_reads_decimal_strings() {
        assert!(!is_non_zero(""));
        assert!(!is_non_zero("0"));
        assert!(!is_non_zero("000"));
        assert!(is_non_zero("1"));
        assert!(is_non_zero("100"));
    }

    #[test]
    fn fixed_1e18_to_bps_divides_by_1e14_and_saturates_at_zero() {
        // 1% = 1e16 in 18-decimal fixed point = 100 bps.
        assert_eq!(fixed_1e18_to_bps(&BigInt::from(10_000_000_000_000_000u64)), 100);
        assert_eq!(fixed_1e18_to_bps(&BigInt::from(99_999_999_999_999u64)), 0);
        // Out of u64 range reads as zero, as v0.8.0's `unwrap_or_default`.
        assert_eq!(fixed_1e18_to_bps(&(BigInt::from(u64::MAX) + 1)), 0);
    }

    #[test]
    fn truncated_fee_matches_v0_8_0_or_fails_where_it_panicked() {
        assert_eq!(truncated_fee(&BigInt::from(4_000_000), "NewFee").unwrap(), 4_000_000);
        // `as u32` truncation of a value that fits into a u64.
        assert_eq!(truncated_fee(&BigInt::from(u64::from(u32::MAX) + 5), "NewFee").unwrap(), 4);
        assert!(truncated_fee(&(BigInt::from(u64::MAX) + 1), "NewFee").is_err());
    }
}
