//! Swaps, swap fees and pool initializations of EVM DEX protocols, normalized
//! to `dex.swaps.v1`.
//!
//! Ported from substreams-evm `dex-swaps` (tag `evm-dex-v0.8.0`, `9663b25`);
//! see the package README for what changed in the port and for the v0.8.0
//! behaviour it keeps on purpose.
//!
//! Most protocols need the pool's tokens and factory, which `dex-pools`'
//! `store_pools` records when the pool is created. [`project`] takes them
//! through [`PoolLookup`], so a host program that decodes blocks itself can
//! run the same decoding without the Substreams runtime; the WASM-only
//! `map_events` handler reads them from the store.

mod aerodrome;
mod balancer;
mod bancor;
mod cow;
mod curvefi;
mod diagnostics;
mod dodo;
mod kyber_elastic;
mod logs;
mod sunpump;
mod traderjoe;
mod uniswap_v1;
mod uniswap_v2;
mod uniswap_v3;
mod uniswap_v4;
mod utils;
mod woofi;

use std::collections::HashMap;
use std::hash::BuildHasher;

use common::create::{CreateLog, CreateTransaction};
pub use diagnostics::{CandidateCounts, SwapDiagnostics};
use logs::PoolMetadataMap;
/// The pool metadata `dex-pools` stores, and [`PoolLookup`] returns.
pub use proto::pb::dex::foundational_store::v1::Pool;
/// The `dex.swaps.v1` messages [`project`] returns.
pub use proto::pb::dex::swaps::v1 as pb;
use substreams::errors::Error;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log, TransactionTrace};

/// Pool metadata recorded by `dex-pools` (`store_pools`).
///
/// The key is the 20-byte pool address, except for Uniswap V4 (the 32-byte
/// pool id) and SunPump (the token address). `store_pools` keeps the first
/// value written for a key, in a block no later than the one being decoded:
/// the Substreams store includes pools created in the same block.
pub trait PoolLookup {
    /// The pool stored under `address`, if any.
    fn get_pool(&self, address: &[u8]) -> Option<Pool>;
}

impl<S: BuildHasher> PoolLookup for HashMap<Vec<u8>, Pool, S> {
    fn get_pool(&self, address: &[u8]) -> Option<Pool> {
        self.get(address).cloned()
    }
}

/// Decodes the swaps, swap fees and pool initializations of `block`'s
/// successful transactions. Transactions without any are left out.
///
/// # Errors
///
/// Where v0.8.0 panicked and so failed the block: a successful transaction
/// without a receipt, or a fee that does not fit into a `u64` in a Curve
/// `CommitNewFee`, `NewFee`, `PlainPoolDeployed` or `MetaPoolDeployed`, or a
/// SunPump `PurchaseFeeSet` or `SaleFeeSet` log.
pub fn project<P: PoolLookup + ?Sized>(block: &Block, pools: &P) -> Result<pb::Events, Error> {
    project_with_diagnostics(block, pools).map(|(events, _)| events)
}

/// Projects the same events as [`project`], with bounded observations from the
/// existing ABI decoder branches. No extra decoding or pool lookups are run.
///
/// Counts describe matched candidates and metadata context, not authenticated
/// pools or complete protocol coverage. Fees and initializations are excluded;
/// a block error returns no partial events or diagnostics. See [`CandidateCounts`].
pub fn project_with_diagnostics<P: PoolLookup + ?Sized>(block: &Block, pools: &P) -> Result<(pb::Events, SwapDiagnostics), Error> {
    // v0.8.0 read every successful transaction's receipt, panicking when one
    // was missing; fail before decoding instead.
    if block.transactions().any(|tx| tx.receipt.is_unset()) {
        return Err(Error::msg("successful transaction has no receipt"));
    }
    let pools = logs::get_pools_by_address(pools, &logs::collect_log_addresses(block));
    let mut transactions = Vec::new();
    let mut diagnostics = SwapDiagnostics::default();
    for tx in block.transactions() {
        transactions.extend(process_transaction(tx, &pools, &mut diagnostics)?);
    }
    Ok((pb::Events { transactions }, diagnostics))
}

fn process_transaction(tx: &TransactionTrace, pools: &PoolMetadataMap, diagnostics: &mut SwapDiagnostics) -> Result<Option<pb::Transaction>, Error> {
    let mut transaction = pb::Transaction::create_transaction(tx);
    let logs_with_calls: Vec<(&Log, Option<&Call>)> = if tx.calls.is_empty() {
        tx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
    } else {
        tx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
    };

    for (log, call) in logs_with_calls {
        for event in decode_log(tx, log, pools, diagnostics)? {
            transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
        }
    }

    if transaction.logs.is_empty() {
        return Ok(None);
    }

    Ok(Some(transaction))
}

/// Every protocol decoder runs on every log, so a log whose signature two
/// protocols share yields an event for each (see the README).
fn decode_log(tx: &TransactionTrace, log: &Log, pools: &PoolMetadataMap, diagnostics: &mut SwapDiagnostics) -> Result<Vec<pb::log::Log>, Error> {
    let mut events = Vec::new();

    events.extend(uniswap_v1::decode_log(tx, log, pools, diagnostics.for_protocol_mut(pb::Protocol::UniswapV1)));
    events.extend(uniswap_v2::decode_log(tx, log, pools, diagnostics.for_protocol_mut(pb::Protocol::UniswapV2)));
    events.extend(uniswap_v3::decode_log(tx, log, pools, diagnostics.for_protocol_mut(pb::Protocol::UniswapV3)));
    events.extend(uniswap_v4::decode_log(tx, log, pools, diagnostics.for_protocol_mut(pb::Protocol::UniswapV4)));
    events.extend(curvefi::decode_log(tx, log, pools, diagnostics.for_protocol_mut(pb::Protocol::Curvefi))?);
    events.extend(balancer::decode_log(tx, log, pools, diagnostics.for_protocol_mut(pb::Protocol::Balancer)));
    events.extend(bancor::decode_log(tx, log, pools, diagnostics.for_protocol_mut(pb::Protocol::Bancor))?);
    events.extend(cow::decode_log(tx, log, diagnostics.for_protocol_mut(pb::Protocol::Cow)));
    events.extend(aerodrome::decode_log(tx, log, pools, diagnostics.for_protocol_mut(pb::Protocol::Aerodrome)));
    events.extend(dodo::decode_log(tx, log, diagnostics.for_protocol_mut(pb::Protocol::Dodo)));
    events.extend(woofi::decode_log(tx, log, diagnostics.for_protocol_mut(pb::Protocol::Woofi)));
    events.extend(traderjoe::decode_log(tx, log, pools, diagnostics.for_protocol_mut(pb::Protocol::Traderjoe)));
    events.extend(kyber_elastic::decode_log(
        tx,
        log,
        pools,
        diagnostics.for_protocol_mut(pb::Protocol::KyberElastic),
    ));
    events.extend(sunpump::decode_log(tx, log, pools, diagnostics.for_protocol_mut(pb::Protocol::Sunpump))?);

    Ok(events)
}

// Only the WASM build exports the package's map_events.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;
    use substreams::store::{StoreGet, StoreGetProto};
    use substreams::Hex;

    /// `dex-pools`' `store_pools`, keyed by the hex-encoded address
    /// (lowercase, no `0x`).
    struct StorePools(StoreGetProto<Pool>);

    impl PoolLookup for StorePools {
        fn get_pool(&self, address: &[u8]) -> Option<Pool> {
            self.0.get_last(Hex::encode(address))
        }
    }

    #[substreams::handlers::map]
    fn map_events(block: Block, store: StoreGetProto<Pool>) -> Result<pb::Events, Error> {
        project(&block, &StorePools(store))
    }
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;
