//! Uniswap V3 pool and factory events, as `uniswap.v3.Events`.
//!
//! Ported from substreams-evm `dex/uniswap-v3` (tag `evm-dex-v0.8.0`,
//! `9663b25`); the package README lists what changed and the v0.8.0
//! behaviour it keeps, such as negative ticks emitted as 0. [`project`] runs
//! the decoding without the Substreams runtime, for a host program that
//! decodes blocks itself; the `map_events` handler exists only in the WASM
//! build.
//!
//! The `uniswap.v3` protobuf package also holds `dex/pool-state`'s
//! `BlockPoolChanges` and its parts; this package emits only `Events`.

use common::create::{CreateLog, CreateTransaction};
use common::{bigint_to_i32, bigint_to_u64};
/// The `uniswap.v3` messages [`project`] returns.
pub use proto::pb::uniswap::v3 as pb;
use substreams::errors::Error;
use substreams_abis::dex::uniswap::v3 as uniswap;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log};
use substreams_ethereum::Event;

/// Decodes the Uniswap V3 events of `block`'s successful transactions, from
/// any emitter: pool `Swap`, `Initialize`, `Mint`, `Collect`, `Burn`,
/// `Flash`, `IncreaseObservationCardinalityNext`, `SetFeeProtocol` and
/// `CollectProtocol`, and factory `PoolCreated`, `OwnerChanged` and
/// `FeeAmountEnabled`. Transactions without one are left out.
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
    let mut total_mints = 0;
    let mut total_burns = 0;
    let mut total_collects = 0;
    let mut total_flashes = 0;
    let mut total_pool_created = 0;
    let mut total_initialize = 0;
    let mut total_increase_observation = 0;
    let mut total_set_fee_protocol = 0;
    let mut total_collect_protocol = 0;
    let mut total_owner_changed = 0;
    let mut total_fee_amount_enabled = 0;

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        let logs_with_calls: Vec<(&Log, Option<&Call>)> = if trx.calls.is_empty() {
            trx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
        } else {
            trx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
        };
        for (log, call) in logs_with_calls {
            // Swap event
            if let Some(event) = uniswap::pool::events::Swap::match_and_decode(log) {
                total_swaps += 1;
                let event = pb::log::Log::from(pb::Swap {
                    sender: event.sender.to_vec(),
                    recipient: event.recipient.to_vec(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                    sqrt_price_x96: event.sqrt_price_x96.to_string(),
                    liquidity: event.liquidity.to_string(),
                    tick: bigint_to_i32(&event.tick).unwrap_or_default(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Initialize event
            if let Some(event) = uniswap::pool::events::Initialize::match_and_decode(log) {
                total_initialize += 1;
                let event = pb::log::Log::from(pb::Initialize {
                    sqrt_price_x96: event.sqrt_price_x96.to_string(),
                    tick: bigint_to_i32(&event.tick).unwrap_or_default(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Mint event
            if let Some(event) = uniswap::pool::events::Mint::match_and_decode(log) {
                total_mints += 1;
                let event = pb::log::Log::from(pb::Mint {
                    sender: event.sender.to_vec(),
                    owner: event.owner.to_vec(),
                    tick_lower: bigint_to_i32(&event.tick_lower).unwrap_or_default(),
                    tick_upper: bigint_to_i32(&event.tick_upper).unwrap_or_default(),
                    amount: event.amount.to_string(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Collect event
            if let Some(event) = uniswap::pool::events::Collect::match_and_decode(log) {
                total_collects += 1;
                let event = pb::log::Log::from(pb::Collect {
                    owner: event.owner.to_vec(),
                    recipient: event.recipient.to_vec(),
                    tick_lower: bigint_to_i32(&event.tick_lower).unwrap_or_default(),
                    tick_upper: bigint_to_i32(&event.tick_upper).unwrap_or_default(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Burn event
            if let Some(event) = uniswap::pool::events::Burn::match_and_decode(log) {
                total_burns += 1;
                let event = pb::log::Log::from(pb::Burn {
                    owner: event.owner.to_vec(),
                    tick_lower: bigint_to_i32(&event.tick_lower).unwrap_or_default(),
                    tick_upper: bigint_to_i32(&event.tick_upper).unwrap_or_default(),
                    amount: event.amount.to_string(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Flash event
            if let Some(event) = uniswap::pool::events::Flash::match_and_decode(log) {
                total_flashes += 1;
                let event = pb::log::Log::from(pb::Flash {
                    sender: event.sender.to_vec(),
                    recipient: event.recipient.to_vec(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                    paid0: event.paid0.to_string(),
                    paid1: event.paid1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // IncreaseObservationCardinalityNext event
            if let Some(event) = uniswap::pool::events::IncreaseObservationCardinalityNext::match_and_decode(log) {
                total_increase_observation += 1;
                let event = pb::log::Log::from(pb::IncreaseObservationCardinalityNext {
                    observation_cardinality_next_old: bigint_to_u64(&event.observation_cardinality_next_old).unwrap_or_default(),
                    observation_cardinality_next_new: bigint_to_u64(&event.observation_cardinality_next_new).unwrap_or_default(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // SetFeeProtocol event
            if let Some(event) = uniswap::pool::events::SetFeeProtocol::match_and_decode(log) {
                total_set_fee_protocol += 1;
                let event = pb::log::Log::from(pb::SetFeeProtocol {
                    fee_protocol0_old: bigint_to_u64(&event.fee_protocol0_old).unwrap_or_default(),
                    fee_protocol1_old: bigint_to_u64(&event.fee_protocol1_old).unwrap_or_default(),
                    fee_protocol0_new: bigint_to_u64(&event.fee_protocol0_new).unwrap_or_default(),
                    fee_protocol1_new: bigint_to_u64(&event.fee_protocol1_new).unwrap_or_default(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // CollectProtocol event
            if let Some(event) = uniswap::pool::events::CollectProtocol::match_and_decode(log) {
                total_collect_protocol += 1;
                let event = pb::log::Log::from(pb::CollectProtocol {
                    sender: event.sender.to_vec(),
                    recipient: event.recipient.to_vec(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // PoolCreated event
            if let Some(event) = uniswap::factory::events::PoolCreated::match_and_decode(log) {
                total_pool_created += 1;
                let event = pb::log::Log::from(pb::PoolCreated {
                    pool: event.pool.to_vec(),
                    token0: event.token0.to_vec(),
                    token1: event.token1.to_vec(),
                    fee: bigint_to_u64(&event.fee).unwrap_or_default(),
                    tick_spacing: bigint_to_i32(&event.tick_spacing).unwrap_or_default(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // OwnerChanged event
            if let Some(event) = uniswap::factory::events::OwnerChanged::match_and_decode(log) {
                total_owner_changed += 1;
                let event = pb::log::Log::from(pb::OwnerChanged {
                    old_owner: event.old_owner.to_vec(),
                    new_owner: event.new_owner.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // FeeAmountEnabled event
            if let Some(event) = uniswap::factory::events::FeeAmountEnabled::match_and_decode(log) {
                total_fee_amount_enabled += 1;
                let event = pb::log::Log::from(pb::FeeAmountEnabled {
                    fee: bigint_to_u64(&event.fee).unwrap_or_default(),
                    tick_spacing: bigint_to_i32(&event.tick_spacing).unwrap_or_default(),
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
    substreams::log::info!("Total Collect events: {}", total_collects);
    substreams::log::info!("Total Flash events: {}", total_flashes);
    substreams::log::info!("Total PoolCreated events: {}", total_pool_created);
    substreams::log::info!("Total Initialize events: {}", total_initialize);
    substreams::log::info!("Total IncreaseObservationCardinalityNext events: {}", total_increase_observation);
    substreams::log::info!("Total SetFeeProtocol events: {}", total_set_fee_protocol);
    substreams::log::info!("Total CollectProtocol events: {}", total_collect_protocol);
    substreams::log::info!("Total OwnerChanged events: {}", total_owner_changed);
    substreams::log::info!("Total FeeAmountEnabled events: {}", total_fee_amount_enabled);
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
