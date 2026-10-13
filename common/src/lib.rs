//! The parts of substreams-evm's `common` crate (tag `evm-dex-v0.8.0`,
//! `9663b25`) that the DEX packages use: the transaction, log and call
//! templates of `dex.swaps.v1` and of the per-protocol packages,
//! [`bigint_to_u64`] and [`bigint_to_i32`].
//!
//! The Tron Base58 and hex encodings and the ClickHouse column helpers live in
//! `evm-dex`, their only user; the templates of the token packages were not
//! ported, since no package of this repository uses them.

pub mod create;
#[cfg(feature = "testing")]
pub mod testing;

use substreams::{errors::Error, log, scalar::BigInt};

/// Converts a non-negative integer that fits into a `u64`, as v0.8.0 did.
///
/// Returns `None`, and logs why, for a negative value or one above
/// `u64::MAX`; callers that need a value use `unwrap_or_default()`.
pub fn bigint_to_u64(bigint: &BigInt) -> Option<u64> {
    if bigint.lt(&BigInt::zero()) {
        log::info!("bigint_to_u64: value is negative");
        return None;
    }
    if bigint.gt(&BigInt::from(u64::MAX)) {
        log::info!("bigint_to_u64: value is greater than u64::MAX");
        return None;
    }
    Some(bigint.to_u64())
}

/// Converts a non-negative integer that fits into an `i32`, as v0.8.0 did.
///
/// Returns `None`, and logs why, for a negative value or one above
/// `i32::MAX`; callers use `unwrap_or_default()`, so a negative `int24` tick
/// or tick spacing becomes 0. substreams-evm fixed this after v0.8.0
/// (`b596c2d`, accepting `i32::MIN..=i32::MAX`); the per-protocol packages
/// keep v0.8.0's output until the owner approves the change.
pub fn bigint_to_i32(bigint: &BigInt) -> Option<i32> {
    if bigint.lt(&BigInt::zero()) {
        log::info!("bigint_to_i32: value is negative");
        return None;
    }
    if bigint.gt(&BigInt::from(i32::MAX)) {
        log::info!("bigint_to_i32: value is greater than i32::MAX");
        return None;
    }
    Some(bigint.to_i32())
}

/// `BigInt::to_u64()`, returning an error where it panics.
///
/// v0.8.0 called `to_u64()` on some decoded integers (TraderJoe bin ids, Kyber
/// `swapFeeUnits`, Balancer `kind` and token types, SunPump legacy NFT
/// supplies), which panics, failing the block, on a value above `u64::MAX`:
/// an ABI word can carry one whatever the declared width. This fails the
/// block with an error naming `field` instead.
pub fn try_to_u64(value: &BigInt, field: &str) -> Result<u64, Error> {
    if value.lt(&BigInt::zero()) || value.gt(&BigInt::from(u64::MAX)) {
        return Err(Error::msg(format!("{field}: BigInt '{value}' is too large to fit into u64")));
    }
    Ok(value.to_u64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigint_to_u64_rejects_negative_and_overflowing_values() {
        assert_eq!(bigint_to_u64(&BigInt::zero()), Some(0));
        assert_eq!(bigint_to_u64(&BigInt::from(3000)), Some(3000));
        assert_eq!(bigint_to_u64(&BigInt::from(u64::MAX)), Some(u64::MAX));
        assert_eq!(bigint_to_u64(&BigInt::from(-1)), None);
        assert_eq!(bigint_to_u64(&(BigInt::from(u64::MAX) + 1)), None);
    }

    #[test]
    fn bigint_to_i32_rejects_negative_and_overflowing_values() {
        assert_eq!(bigint_to_i32(&BigInt::zero()), Some(0));
        assert_eq!(bigint_to_i32(&BigInt::from(887272)), Some(887272));
        assert_eq!(bigint_to_i32(&BigInt::from(i32::MAX)), Some(i32::MAX));
        // v0.8.0: a negative tick is rejected, so callers emit 0.
        assert_eq!(bigint_to_i32(&BigInt::from(-1)), None);
        assert_eq!(bigint_to_i32(&BigInt::from(-887272)), None);
        assert_eq!(bigint_to_i32(&(BigInt::from(i32::MAX) + 1)), None);
    }

    #[test]
    fn try_to_u64_fails_where_to_u64_panics() {
        assert_eq!(try_to_u64(&BigInt::zero(), "id").unwrap(), 0);
        assert_eq!(try_to_u64(&BigInt::from(u64::MAX), "id").unwrap(), u64::MAX);
        let error = try_to_u64(&(BigInt::from(u64::MAX) + 1), "Swap.id").unwrap_err();
        assert_eq!(error.to_string(), "Swap.id: BigInt '18446744073709551616' is too large to fit into u64");
        assert!(try_to_u64(&BigInt::from(-1), "id").is_err());
    }
}
