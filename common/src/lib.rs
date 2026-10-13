//! The parts of substreams-evm's `common` crate (tag `evm-dex-v0.8.0`,
//! `9663b25`) that `dex-swaps` uses: the transaction, log and call templates
//! of `dex.swaps.v1` and [`bigint_to_u64`].
//!
//! The Tron Base58 and hex encodings, the ClickHouse helpers and the templates
//! of the per-protocol and token packages were not ported: no package of this
//! repository uses them yet.

pub mod create;

use substreams::{log, scalar::BigInt};

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
}
