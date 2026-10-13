//! KyberSwap Elastic pool and factory events, as `kyber_elastic.v1.Events`.
//!
//! Ported from substreams-evm `dex/kyber-elastic` (tag `evm-dex-v0.8.0`,
//! `9663b25`); the package README lists what changed and the v0.8.0
//! behaviour it keeps, such as negative ticks emitted as 0. [`project`] runs
//! the decoding without the Substreams runtime, for a host program that
//! decodes blocks itself; the `map_events` handler exists only in the WASM
//! build.

use common::bigint_to_i32;
use common::create::{CreateLog, CreateTransaction};
use common::try_to_u64;
/// The `kyber_elastic.v1` messages [`project`] returns.
pub use proto::pb::kyber_elastic::v1 as pb;
use substreams::errors::Error;
use substreams_abis::dex::kyber::elastic;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log};
use substreams_ethereum::Event;

/// Decodes the KyberSwap Elastic events of `block`'s successful
/// transactions, from any emitter: pool `Swap`, `Mint`, `Burn`,
/// `BurnRTokens`, `Flash` and `Initialize`, and factory `PoolCreated`.
/// Transactions without one are left out.
///
/// # Errors
///
/// Where v0.8.0 panicked, failing the block: a successful transaction
/// without a receipt, or a `PoolCreated` `swapFeeUnits` above `u64::MAX`.
pub fn project(block: &Block) -> Result<pb::Events, Error> {
    // v0.8.0 read every successful transaction's receipt, panicking when one
    // was missing; fail before decoding instead.
    if block.transactions().any(|trx| trx.receipt.is_unset()) {
        return Err(Error::msg("successful transaction has no receipt"));
    }

    let mut events = pb::Events::default();
    let mut total_swaps = 0;
    let mut total_mints = 0;
    let mut total_burns = 0;
    let mut total_burn_r_tokens = 0;
    let mut total_flash = 0;
    let mut total_initialize = 0;
    let mut total_pool_created = 0;

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        let logs_with_calls: Vec<(&Log, Option<&Call>)> = if trx.calls.is_empty() {
            trx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
        } else {
            trx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
        };
        for (log, call) in logs_with_calls {
            // Swap event (Pool)
            if let Some(event) = elastic::pool::events::Swap::match_and_decode(log) {
                total_swaps += 1;
                let event = pb::log::Log::from(pb::Swap {
                    sender: event.sender.to_vec(),
                    recipient: event.recipient.to_vec(),
                    delta_qty0: event.delta_qty0.to_string(),
                    delta_qty1: event.delta_qty1.to_string(),
                    sqrt_p: event.sqrt_p.to_string(),
                    liquidity: event.liquidity.to_string(),
                    current_tick: bigint_to_i32(&event.current_tick).unwrap_or_default(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Mint event (Pool)
            if let Some(event) = elastic::pool::events::Mint::match_and_decode(log) {
                total_mints += 1;
                let event = pb::log::Log::from(pb::Mint {
                    sender: event.sender.to_vec(),
                    owner: event.owner.to_vec(),
                    tick_lower: bigint_to_i32(&event.tick_lower).unwrap_or_default(),
                    tick_upper: bigint_to_i32(&event.tick_upper).unwrap_or_default(),
                    qty: event.qty.to_string(),
                    qty0: event.qty0.to_string(),
                    qty1: event.qty1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Burn event (Pool)
            if let Some(event) = elastic::pool::events::Burn::match_and_decode(log) {
                total_burns += 1;
                let event = pb::log::Log::from(pb::Burn {
                    owner: event.owner.to_vec(),
                    tick_lower: bigint_to_i32(&event.tick_lower).unwrap_or_default(),
                    tick_upper: bigint_to_i32(&event.tick_upper).unwrap_or_default(),
                    qty: event.qty.to_string(),
                    qty0: event.qty0.to_string(),
                    qty1: event.qty1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // BurnRTokens event (Pool)
            if let Some(event) = elastic::pool::events::BurnRTokens::match_and_decode(log) {
                total_burn_r_tokens += 1;
                let event = pb::log::Log::from(pb::BurnRTokens {
                    owner: event.owner.to_vec(),
                    qty: event.qty.to_string(),
                    qty0: event.qty0.to_string(),
                    qty1: event.qty1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Flash event (Pool)
            if let Some(event) = elastic::pool::events::Flash::match_and_decode(log) {
                total_flash += 1;
                let event = pb::log::Log::from(pb::Flash {
                    sender: event.sender.to_vec(),
                    recipient: event.recipient.to_vec(),
                    qty0: event.qty0.to_string(),
                    qty1: event.qty1.to_string(),
                    paid0: event.paid0.to_string(),
                    paid1: event.paid1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Initialize event (Pool)
            if let Some(event) = elastic::pool::events::Initialize::match_and_decode(log) {
                total_initialize += 1;
                let event = pb::log::Log::from(pb::Initialize {
                    sqrt_p: event.sqrt_p.to_string(),
                    tick: bigint_to_i32(&event.tick).unwrap_or_default(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // PoolCreated event (Factory)
            if let Some(event) = elastic::factory::events::PoolCreated::match_and_decode(log) {
                total_pool_created += 1;
                let event = pb::log::Log::from(pb::PoolCreated {
                    token0: event.token0.to_vec(),
                    token1: event.token1.to_vec(),
                    swap_fee_units: try_to_u64(&event.swap_fee_units, "PoolCreated.swapFeeUnits")? as u32,
                    tick_distance: bigint_to_i32(&event.tick_distance).unwrap_or_default(),
                    pool: event.pool.to_vec(),
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
    substreams::log::info!("Total Mint events: {}", total_mints);
    substreams::log::info!("Total Burn events: {}", total_burns);
    substreams::log::info!("Total BurnRTokens events: {}", total_burn_r_tokens);
    substreams::log::info!("Total Flash events: {}", total_flash);
    substreams::log::info!("Total Initialize events: {}", total_initialize);
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
