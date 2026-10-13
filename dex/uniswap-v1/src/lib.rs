//! Uniswap V1 exchange and factory events, with JustSwap's `TrxPurchase`, as
//! `uniswap.v1.Events`.
//!
//! Ported from substreams-evm `dex/uniswap-v1` (tag `evm-dex-v0.8.0`,
//! `9663b25`); the package README lists what changed. [`project`] runs the
//! decoding without the Substreams runtime, for a host program that decodes
//! blocks itself; the `map_events` handler exists only in the WASM build.

use common::create::{CreateLog, CreateTransaction};
/// The `uniswap.v1` messages [`project`] returns.
pub use proto::pb::uniswap::v1 as pb;
use substreams::errors::Error;
use substreams_abis::dex::justswap::v1 as justswap;
use substreams_abis::dex::uniswap::v1 as uniswap;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log};
use substreams_ethereum::Event;

/// Decodes the Uniswap V1 events of `block`'s successful transactions, from
/// any emitter: exchange `TokenPurchase`, `EthPurchase` (and JustSwap's
/// `TrxPurchase`, as an `EthPurchase`), `AddLiquidity` and `RemoveLiquidity`,
/// and factory `NewExchange`. Transactions without one are left out.
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
    let mut total_token_purchases = 0;
    let mut total_eth_purchases = 0;
    let mut total_add_liquidity = 0;
    let mut total_remove_liquidity = 0;
    let mut total_new_exchanges = 0;

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        let logs_with_calls: Vec<(&Log, Option<&Call>)> = if trx.calls.is_empty() {
            trx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
        } else {
            trx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
        };
        for (log, call) in logs_with_calls {
            // TokenPurchase event
            if let Some(event) = uniswap::exchange::events::TokenPurchase::match_and_decode(log) {
                total_token_purchases += 1;
                let event = pb::log::Log::from(pb::TokenPurchase {
                    buyer: event.buyer.to_vec(),
                    eth_sold: event.eth_sold.to_string(),
                    tokens_bought: event.tokens_bought.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // EthPurchase event
            if let Some(event) = uniswap::exchange::events::EthPurchase::match_and_decode(log) {
                total_eth_purchases += 1;
                let event = pb::log::Log::from(pb::EthPurchase {
                    buyer: event.buyer.to_vec(),
                    tokens_sold: event.tokens_sold.to_string(),
                    eth_bought: event.eth_bought.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // (Justswap TVM) TrxPurchase event
            if let Some(event) = justswap::exchange::events::TrxPurchase::match_and_decode(log) {
                total_eth_purchases += 1;
                let event = pb::log::Log::from(pb::EthPurchase {
                    buyer: event.buyer.to_vec(),
                    tokens_sold: event.tokens_sold.to_string(),
                    eth_bought: event.trx_bought.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // AddLiquidity event
            if let Some(event) = uniswap::exchange::events::AddLiquidity::match_and_decode(log) {
                total_add_liquidity += 1;
                let event = pb::log::Log::from(pb::AddLiquidity {
                    provider: event.provider.to_vec(),
                    eth_amount: event.eth_amount.to_string(),
                    token_amount: event.token_amount.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // RemoveLiquidity event
            if let Some(event) = uniswap::exchange::events::RemoveLiquidity::match_and_decode(log) {
                total_remove_liquidity += 1;
                let event = pb::log::Log::from(pb::RemoveLiquidity {
                    provider: event.provider.to_vec(),
                    eth_amount: event.eth_amount.to_string(),
                    token_amount: event.token_amount.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // NewExchange event
            if let Some(event) = uniswap::factory::events::NewExchange::match_and_decode(log) {
                total_new_exchanges += 1;
                let event = pb::log::Log::from(pb::NewExchange {
                    token: event.token.to_vec(),
                    exchange: event.exchange.to_vec(),
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
    substreams::log::info!("Total TokenPurchase events: {}", total_token_purchases);
    substreams::log::info!("Total EthPurchase events: {}", total_eth_purchases);
    substreams::log::info!("Total AddLiquidity events: {}", total_add_liquidity);
    substreams::log::info!("Total RemoveLiquidity events: {}", total_remove_liquidity);
    substreams::log::info!("Total NewExchange events: {}", total_new_exchanges);
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
