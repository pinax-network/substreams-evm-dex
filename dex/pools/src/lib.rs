//! DEX pool metadata from pool-creation events, kept in a `set_if_not_exists`
//! store keyed by pool address.
//!
//! Ported from substreams-evm `dex-swaps-pools` (tag `evm-dex-v0.8.0`,
//! `9663b25`); see the package README for what changed in the port.
//!
//!   - key   = hex-encoded pool address (lowercase, no `0x`); the Uniswap V4
//!     pool id for V4 and the token address for SunPump
//!   - value = [`Pool`] `{ tokens[], factory }`
//!   - policy = `set_if_not_exists` (the first writer wins)
//!
//! Pool data is extracted directly from a single `sf.ethereum.type.v2.Block`
//! with the `substreams-abis` event decoders. The payload is restricted to
//! shared pool metadata: `tokens[]` plus `factory` when available.
//! Protocol-specific init fields (Aerodrome `stable`, TraderJoe `bin_step`,
//! Kyber `swap_fee_units`/`tick_distance`, ...) remain on the original
//! pool-creation events.
//!
//! [`project`] runs the extraction without the Substreams runtime, for a host
//! program that decodes blocks itself. The `store_pools` and `map_pools`
//! handlers exist only in the WASM build.

mod aerodrome;
mod balancer;
mod bancor;
mod curvefi;
mod kyber;
mod sunpump;
mod traderjoe;
mod uniswap;

use std::collections::HashSet;

pub use proto::pb::dex::foundational_store::v1::{Pool, PoolCreated, Pools};
use substreams::errors::Error;
use substreams::pb::substreams::store_delta::Operation;
use substreams::store::{DeltaProto, Deltas};
use substreams::Hex;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, CallType, Log, TransactionTrace};

/// A decoded pool ready to be written to the store.
pub(crate) struct PoolEntry {
    pub address: Vec<u8>,
    pub tokens: Vec<Vec<u8>>,
    pub factory: Vec<u8>,
}

/// The pools first created in `block`, in the order `store_pools` writes
/// them, keeping the first write of each pool.
///
/// This is what `map_pools` emits for the block when the store holds none of
/// these pools yet. A host keeping its own store drops the pools it already
/// holds: the first writer wins.
///
/// # Errors
///
/// When a successful transaction has neither call traces nor a receipt.
/// v0.8.0 panicked there, failing the block.
pub fn project(block: &Block) -> Result<Pools, Error> {
    let mut seen = HashSet::new();
    let pools = entries(block)?
        .into_iter()
        .filter_map(|entry| {
            let address = Hex::encode(&entry.address);
            seen.insert(address.clone()).then(|| PoolCreated {
                address,
                pool: Pool {
                    tokens: entry.tokens,
                    factory: entry.factory,
                }
                .into(),
            })
        })
        .collect();
    Ok(Pools { pools })
}

/// What `map_pools` emits for a block's `store_pools` deltas: the pools first
/// seen in the block. Since `store_pools` uses `set_if_not_exists`, only the
/// first write per pool produces a `Create` delta; later blocks never re-emit
/// it.
pub fn pools_from_deltas(deltas: Deltas<DeltaProto<Pool>>) -> Pools {
    let pools = deltas
        .deltas
        .into_iter()
        .filter(|delta| delta.operation == Operation::Create)
        .map(|delta| PoolCreated {
            address: delta.key,
            pool: delta.new_value.into(),
        })
        .collect();
    Pools { pools }
}

/// Every pool entry of the block in write order, duplicates included. Empty
/// addresses are skipped, as v0.8.0's `set_pool` did.
fn entries(block: &Block) -> Result<Vec<PoolEntry>, Error> {
    let mut entries = Vec::new();
    for trx in block.transactions() {
        // CurveFi direct deployments expose pool metadata via the constructor calldata,
        // not via a log — decode it once per transaction before walking the logs.
        entries.extend(curvefi::collect_curvefi_init(trx));

        // CurveFi factory events identify the pool by the freshly-CREATE'd contract address,
        // which lives in the call traces rather than the event itself.
        let create_address = get_create_address(trx);

        for log in trx_logs(trx)? {
            entries.extend(collect_log(log, create_address.as_deref()));
        }
    }
    entries.retain(|entry| !entry.address.is_empty());
    Ok(entries)
}

/// Decode the (at most one) supported pool-creation event from a single log.
fn collect_log(log: &Log, create_address: Option<&[u8]>) -> Option<PoolEntry> {
    uniswap::collect_uniswap_v1(log)
        .or_else(|| uniswap::collect_uniswap_v2(log))
        .or_else(|| uniswap::collect_uniswap_v3(log))
        .or_else(|| uniswap::collect_uniswap_v4(log))
        .or_else(|| aerodrome::collect_aerodrome(log))
        .or_else(|| kyber::collect_kyber_elastic(log))
        .or_else(|| traderjoe::collect_traderjoe(log))
        .or_else(|| balancer::collect_balancer(log))
        .or_else(|| bancor::collect_bancor(log))
        .or_else(|| sunpump::collect_sunpump(log))
        .or_else(|| curvefi::collect_curvefi_factory(log, create_address))
}

// ── Block / transaction helpers ───────────────────────────────────────────────────────────

/// A transaction's logs: those of its non-reverted calls in ordinal order, or
/// its receipt's logs when it has no call traces.
fn trx_logs(trx: &TransactionTrace) -> Result<Vec<&Log>, Error> {
    if trx.calls.is_empty() {
        let receipt = trx
            .receipt
            .as_option()
            .ok_or_else(|| Error::msg("successful transaction has neither call traces nor a receipt"))?;
        Ok(receipt.logs.iter().collect())
    } else {
        Ok(trx.logs_with_calls().map(|(log, _)| log).collect())
    }
}

/// Address of the first contract CREATE'd in this transaction, if any.
fn get_create_address(trx: &TransactionTrace) -> Option<Vec<u8>> {
    trx.calls
        .iter()
        .find(|call| call.call_type == CallType::Create)
        .map(|call| call.address.to_vec())
}

// Only the WASM build exports the package's modules.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;
    use substreams::store::{StoreNew, StoreSetIfNotExists, StoreSetIfNotExistsProto};

    /// The ordinal is always `0`: only the first value seen for a pool is
    /// stored and never overwritten, so intra-block ordering is not tracked.
    #[substreams::handlers::store]
    fn store_pools(block: Block, store: StoreSetIfNotExistsProto<Pool>) {
        // A store handler cannot return an error: fail the block as v0.8.0's
        // panic did.
        let entries = entries(&block).unwrap_or_else(|error| panic!("{error}"));
        for entry in entries {
            store.set_if_not_exists(
                0,
                Hex::encode(&entry.address),
                &Pool {
                    tokens: entry.tokens,
                    factory: entry.factory,
                },
            );
        }
    }

    #[substreams::handlers::map]
    fn map_pools(deltas: Deltas<DeltaProto<Pool>>) -> Pools {
        pools_from_deltas(deltas)
    }
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;
