//! Uniswap V4 PoolManager events, as `uniswap.v4.Events`.
//!
//! Ported from substreams-evm `dex/uniswap-v4` (tag `evm-dex-v0.8.0`,
//! `9663b25`); the package README lists what changed and the v0.8.0
//! behaviour it keeps, such as negative ticks emitted as 0. [`project`] runs
//! the decoding without the Substreams runtime, for a host program that
//! decodes blocks itself; the `map_events` handler exists only in the WASM
//! build.

use common::create::{CreateLog, CreateTransaction};
use common::{bigint_to_i32, bigint_to_u64};
/// The `uniswap.v4` messages [`project`] returns.
pub use proto::pb::uniswap::v4 as pb;
use substreams::errors::Error;
use substreams_abis::dex::uniswap::v4 as uniswap;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log};
use substreams_ethereum::Event;

/// Decodes the Uniswap V4 PoolManager events of `block`'s successful
/// transactions, from any emitter: `Swap`, `Initialize`, `ModifyLiquidity`,
/// `Donate`, `ProtocolFeeControllerUpdated` and `ProtocolFeeUpdated`.
/// Transactions without one are left out.
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
    let mut total_initialize = 0;
    let mut total_modify_liquidity = 0;
    let mut total_donate = 0;
    let mut total_protocol_fee_controller_updated = 0;
    let mut total_protocol_fee_updated = 0;

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        let logs_with_calls: Vec<(&Log, Option<&Call>)> = if trx.calls.is_empty() {
            trx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
        } else {
            trx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
        };
        for (log, call) in logs_with_calls {
            // Swap event
            if let Some(event) = uniswap::poolmanager::events::Swap::match_and_decode(log) {
                total_swaps += 1;
                let event = pb::log::Log::from(pb::Swap {
                    id: event.id.to_vec(),
                    sender: event.sender.to_vec(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                    sqrt_price_x96: event.sqrt_price_x96.to_string(),
                    liquidity: event.liquidity.to_string(),
                    tick: bigint_to_i32(&event.tick).unwrap_or_default(),
                    fee: event.fee.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Initialize event
            if let Some(event) = uniswap::poolmanager::events::Initialize::match_and_decode(log) {
                total_initialize += 1;
                let event = pb::log::Log::from(pb::Initialize {
                    id: event.id.to_vec(),
                    currency0: event.currency0.to_vec(),
                    currency1: event.currency1.to_vec(),
                    fee: bigint_to_u64(&event.fee).unwrap_or_default(),
                    tick_spacing: bigint_to_i32(&event.tick_spacing).unwrap_or_default(),
                    hooks: event.hooks.to_vec(),
                    sqrt_price_x96: event.sqrt_price_x96.to_string(),
                    tick: bigint_to_i32(&event.tick).unwrap_or_default(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // ModifyLiquidity event
            if let Some(event) = uniswap::poolmanager::events::ModifyLiquidity::match_and_decode(log) {
                total_modify_liquidity += 1;
                let event = pb::log::Log::from(pb::ModifyLiquidity {
                    id: event.id.to_vec(),
                    sender: event.sender.to_vec(),
                    tick_lower: bigint_to_i32(&event.tick_lower).unwrap_or_default(),
                    tick_upper: bigint_to_i32(&event.tick_upper).unwrap_or_default(),
                    liquidity_delta: event.liquidity_delta.to_string(),
                    salt: event.salt.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Donate event
            if let Some(event) = uniswap::poolmanager::events::Donate::match_and_decode(log) {
                total_donate += 1;
                let event = pb::log::Log::from(pb::Donate {
                    id: event.id.to_vec(),
                    sender: event.sender.to_vec(),
                    amount0: event.amount0.to_string(),
                    amount1: event.amount1.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // ProtocolFeeControllerUpdated event
            if let Some(event) = uniswap::poolmanager::events::ProtocolFeeControllerUpdated::match_and_decode(log) {
                total_protocol_fee_controller_updated += 1;
                let event = pb::log::Log::from(pb::ProtocolFeeControllerUpdated {
                    protocol_fee_controller: event.protocol_fee_controller.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // ProtocolFeeUpdated event
            if let Some(event) = uniswap::poolmanager::events::ProtocolFeeUpdated::match_and_decode(log) {
                total_protocol_fee_updated += 1;
                let event = pb::log::Log::from(pb::ProtocolFeeUpdated {
                    id: event.id.to_vec(),
                    protocol_fee: bigint_to_u64(&event.protocol_fee).unwrap_or_default(),
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
    substreams::log::info!("Total Initialize events: {}", total_initialize);
    substreams::log::info!("Total ModifyLiquidity events: {}", total_modify_liquidity);
    substreams::log::info!("Total Donate events: {}", total_donate);
    substreams::log::info!("Total ProtocolFeeControllerUpdated events: {}", total_protocol_fee_controller_updated);
    substreams::log::info!("Total ProtocolFeeUpdated events: {}", total_protocol_fee_updated);
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
