//! CoW Protocol GPv2Settlement events, as `cow.v1.Events`.
//!
//! Ported from substreams-evm `dex/cow` (tag `evm-dex-v0.8.0`, `9663b25`);
//! the package README lists what changed. [`project`] runs the decoding
//! without the Substreams runtime, for a host program that decodes blocks
//! itself; the `map_events` handler exists only in the WASM build.

use common::create::{CreateLog, CreateTransaction};
/// The `cow.v1` messages [`project`] returns.
pub use proto::pb::cow::v1 as pb;
use substreams::errors::Error;
use substreams_abis::dex::cow::gpv2settlement as cow;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log};
use substreams_ethereum::Event;

/// Decodes the GPv2Settlement events of `block`'s successful transactions,
/// from any emitter: `Interaction`, `OrderInvalidated`, `PreSignature`,
/// `Settlement` and `Trade`. Transactions without one are left out.
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
    let mut total_interactions = 0;
    let mut total_order_invalidated = 0;
    let mut total_pre_signatures = 0;
    let mut total_settlements = 0;
    let mut total_trades = 0;

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        let logs_with_calls: Vec<(&Log, Option<&Call>)> = if trx.calls.is_empty() {
            trx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
        } else {
            trx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
        };
        for (log, call) in logs_with_calls {
            // Interaction event
            if let Some(event) = cow::events::Interaction::match_and_decode(log) {
                total_interactions += 1;
                let event = pb::log::Log::from(pb::Interaction {
                    target: event.target.to_vec(),
                    value: event.value.to_string(),
                    selector: event.selector.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // OrderInvalidated event
            if let Some(event) = cow::events::OrderInvalidated::match_and_decode(log) {
                total_order_invalidated += 1;
                let event = pb::log::Log::from(pb::OrderInvalidated {
                    owner: event.owner.to_vec(),
                    order_uid: event.order_uid.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // PreSignature event
            if let Some(event) = cow::events::PreSignature::match_and_decode(log) {
                total_pre_signatures += 1;
                let event = pb::log::Log::from(pb::PreSignature {
                    owner: event.owner.to_vec(),
                    order_uid: event.order_uid.to_vec(),
                    signed: event.signed,
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Settlement event
            if let Some(event) = cow::events::Settlement::match_and_decode(log) {
                total_settlements += 1;
                let event = pb::log::Log::from(pb::Settlement { solver: event.solver.to_vec() });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Trade event
            if let Some(event) = cow::events::Trade::match_and_decode(log) {
                total_trades += 1;
                let event = pb::log::Log::from(pb::Trade {
                    owner: event.owner.to_vec(),
                    sell_token: event.sell_token.to_vec(),
                    buy_token: event.buy_token.to_vec(),
                    sell_amount: event.sell_amount.to_string(),
                    buy_amount: event.buy_amount.to_string(),
                    fee_amount: event.fee_amount.to_string(),
                    order_uid: event.order_uid.to_vec(),
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
    substreams::log::info!("Total Interaction events: {}", total_interactions);
    substreams::log::info!("Total OrderInvalidated events: {}", total_order_invalidated);
    substreams::log::info!("Total PreSignature events: {}", total_pre_signatures);
    substreams::log::info!("Total Settlement events: {}", total_settlements);
    substreams::log::info!("Total Trade events: {}", total_trades);
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
