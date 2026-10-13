//! Aerodrome/Velodrome (Solidly fork) pool and factory events, as
//! `aerodrome.v1.Events`.
//!
//! Ported from substreams-evm `dex/aerodrome` (tag `evm-dex-v0.8.0`,
//! `9663b25`); the package README lists what changed. [`project`] runs the
//! decoding without the Substreams runtime, for a host program that decodes
//! blocks itself; the `map_events` handler exists only in the WASM build.

use common::create::{CreateLog, CreateTransaction};
/// The `aerodrome.v1` messages [`project`] returns.
pub use proto::pb::aerodrome::v1 as pb;
use substreams::errors::Error;
use substreams_abis::dex::aerodrome;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log};
use substreams_ethereum::Event;

/// Decodes the Aerodrome events of `block`'s successful transactions, from
/// any emitter: pool `Swap`, `Sync`, `Mint`, `Burn`, `Fees` and `Claim`, and
/// factory `PoolCreated`. Transactions without one are left out.
///
/// # Errors
///
/// When a successful transaction has no receipt; v0.8.0 panicked there,
/// failing the block.
pub fn project(block: &Block) -> Result<pb::Events, Error> {
    // v0.8.0 read every successful transaction's receipt, panicking when one
    // was missing; fail before decoding instead.
    if block.transactions().any(|trx| trx.receipt.is_unset()) {
        return Err(Error::msg("successful transaction has no receipt"));
    }

    let mut events = pb::Events::default();
    let mut total_swaps = 0;
    let mut total_syncs = 0;
    let mut total_mints = 0;
    let mut total_burns = 0;
    let mut total_fees = 0;
    let mut total_claims = 0;
    let mut total_pool_created = 0;

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        let logs_with_calls: Vec<(&Log, Option<&Call>)> = if trx.calls.is_empty() {
            trx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
        } else {
            trx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
        };
        for (log, call) in logs_with_calls {
            // Swap event
            if let Some(event) = aerodrome::pool::events::Swap::match_and_decode(log) {
                total_swaps += 1;
                let event = pb::log::Log::from(pb::Swap {
                    sender: event.sender.to_vec(),
                    to: event.to.to_vec(),
                    amount0_in: event.amount0_in.to_string(),
                    amount1_in: event.amount1_in.to_string(),
                    amount0_out: event.amount0_out.to_string(),
                    amount1_out: event.amount1_out.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Sync event
            if let Some(event) = aerodrome::pool::events::Sync::match_and_decode(log) {
                total_syncs += 1;
                let event = pb::log::Log::from(pb::Sync {
                    reserve0: event.reserve0.to_string(),
                    reserve1: event.reserve1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Mint event
            if let Some(event) = aerodrome::pool::events::Mint::match_and_decode(log) {
                total_mints += 1;
                let event = pb::log::Log::from(pb::Mint {
                    sender: event.sender.to_vec(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Burn event
            if let Some(event) = aerodrome::pool::events::Burn::match_and_decode(log) {
                total_burns += 1;
                let event = pb::log::Log::from(pb::Burn {
                    sender: event.sender.to_vec(),
                    to: event.to.to_vec(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Fees event
            if let Some(event) = aerodrome::pool::events::Fees::match_and_decode(log) {
                total_fees += 1;
                let event = pb::log::Log::from(pb::Fees {
                    sender: event.sender.to_vec(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Claim event
            if let Some(event) = aerodrome::pool::events::Claim::match_and_decode(log) {
                total_claims += 1;
                let event = pb::log::Log::from(pb::Claim {
                    sender: event.sender.to_vec(),
                    recipient: event.recipient.to_vec(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // PoolCreated event
            if let Some(event) = aerodrome::poolfactory::events::PoolCreated::match_and_decode(log) {
                total_pool_created += 1;
                let event = pb::log::Log::from(pb::PoolCreated {
                    token0: event.token0.to_vec(),
                    token1: event.token1.to_vec(),
                    stable: event.stable,
                    pool: event.pool.to_vec(),
                    extra_data: event.param4.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }
        }

        if !transaction.logs.is_empty() {
            events.transactions.push(transaction);
        }
    }

    substreams::log::info!("Total Transactions: {}", block.transaction_traces.len());
    substreams::log::info!("Total Events: {}", events.transactions.len());
    substreams::log::info!("Total Swap events: {}", total_swaps);
    substreams::log::info!("Total Sync events: {}", total_syncs);
    substreams::log::info!("Total Mint events: {}", total_mints);
    substreams::log::info!("Total Burn events: {}", total_burns);
    substreams::log::info!("Total Fees events: {}", total_fees);
    substreams::log::info!("Total Claim events: {}", total_claims);
    substreams::log::info!("Total PoolCreated events: {}", total_pool_created);
    Ok(events)
}

// Only the WASM build exports the package's map_events.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;

    #[substreams::handlers::map]
    fn map_events(block: Block) -> Result<pb::Events, Error> {
        project(&block)
    }
}

#[cfg(test)]
mod tests;
