//! Bancor converter, registry and factory events, as `bancor.v1.Events`.
//!
//! Ported from substreams-evm `dex/bancor` (tag `evm-dex-v0.8.0`,
//! `9663b25`); the package README lists what changed. [`project`] runs the
//! decoding without the Substreams runtime, for a host program that decodes
//! blocks itself; the `map_events` handler exists only in the WASM build.

use common::bigint_to_u64;
use common::create::{CreateLog, CreateTransaction};
/// The `bancor.v1` messages [`project`] returns.
pub use proto::pb::bancor::v1 as pb;
use substreams::errors::Error;
use substreams_abis::dex::bancor::bancorconverterfactory;
use substreams_abis::dex::bancor::contractfeatures;
use substreams_abis::dex::bancor::converterfactory;
use substreams_abis::dex::bancor::converterregistry;
use substreams_abis::dex::bancor::standardpoolconverter as bancor;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log};
use substreams_ethereum::Event;

/// Decodes the Bancor events of `block`'s successful transactions, from any
/// emitter: standard pool converter `Activation`, `Conversion`,
/// `ConversionFeeUpdate`, `LiquidityAdded`, `LiquidityRemoved`,
/// `OwnerUpdate` and `TokenRateUpdate`; converter registry anchor,
/// convertible token, liquidity pool and smart token additions and
/// removals; `NewConverter` from both converter factories; and contract
/// features `FeaturesAddition` and `FeaturesRemoval`. Transactions without one
/// are left out.
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
    let mut total_activations = 0;
    let mut total_conversions = 0;
    let mut total_conversion_fee_updates = 0;
    let mut total_liquidity_added = 0;
    let mut total_liquidity_removed = 0;
    let mut total_owner_updates = 0;
    let mut total_token_rate_updates = 0;
    let mut total_converter_anchor_added = 0;
    let mut total_converter_anchor_removed = 0;
    let mut total_convertible_token_added = 0;
    let mut total_convertible_token_removed = 0;
    let mut total_liquidity_pool_added = 0;
    let mut total_liquidity_pool_removed = 0;
    let mut total_smart_token_added = 0;
    let mut total_smart_token_removed = 0;
    let mut total_new_converter = 0;
    let mut total_new_converter_legacy = 0;
    let mut total_features_addition = 0;
    let mut total_features_removal = 0;

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        let logs_with_calls: Vec<(&Log, Option<&Call>)> = if trx.calls.is_empty() {
            trx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
        } else {
            trx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
        };
        for (log, call) in logs_with_calls {
            // Activation event
            if let Some(event) = bancor::events::Activation::match_and_decode(log) {
                total_activations += 1;
                let event = pb::log::Log::from(pb::Activation {
                    converter_type: bigint_to_u64(&event.converter_type).unwrap_or_default() as u32,
                    anchor: event.anchor.to_vec(),
                    activated: event.activated,
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // Conversion event
            if let Some(event) = bancor::events::Conversion::match_and_decode(log) {
                total_conversions += 1;
                let event = pb::log::Log::from(pb::Conversion {
                    source_token: event.source_token.to_vec(),
                    target_token: event.target_token.to_vec(),
                    trader: event.trader.to_vec(),
                    source_amount: event.source_amount.to_string(),
                    target_amount: event.target_amount.to_string(),
                    conversion_fee: event.conversion_fee.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // ConversionFeeUpdate event
            if let Some(event) = bancor::events::ConversionFeeUpdate::match_and_decode(log) {
                total_conversion_fee_updates += 1;
                let event = pb::log::Log::from(pb::ConversionFeeUpdate {
                    prev_fee: bigint_to_u64(&event.prev_fee).unwrap_or_default() as u32,
                    new_fee: bigint_to_u64(&event.new_fee).unwrap_or_default() as u32,
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // LiquidityAdded event
            if let Some(event) = bancor::events::LiquidityAdded::match_and_decode(log) {
                total_liquidity_added += 1;
                let event = pb::log::Log::from(pb::LiquidityAdded {
                    provider: event.provider.to_vec(),
                    reserve_token: event.reserve_token.to_vec(),
                    amount: event.amount.to_string(),
                    new_balance: event.new_balance.to_string(),
                    new_supply: event.new_supply.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // LiquidityRemoved event
            if let Some(event) = bancor::events::LiquidityRemoved::match_and_decode(log) {
                total_liquidity_removed += 1;
                let event = pb::log::Log::from(pb::LiquidityRemoved {
                    provider: event.provider.to_vec(),
                    reserve_token: event.reserve_token.to_vec(),
                    amount: event.amount.to_string(),
                    new_balance: event.new_balance.to_string(),
                    new_supply: event.new_supply.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // OwnerUpdate event
            if let Some(event) = bancor::events::OwnerUpdate::match_and_decode(log) {
                total_owner_updates += 1;
                let event = pb::log::Log::from(pb::OwnerUpdate {
                    prev_owner: event.prev_owner.to_vec(),
                    new_owner: event.new_owner.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // TokenRateUpdate event
            if let Some(event) = bancor::events::TokenRateUpdate::match_and_decode(log) {
                total_token_rate_updates += 1;
                let event = pb::log::Log::from(pb::TokenRateUpdate {
                    token1: event.token1.to_vec(),
                    token2: event.token2.to_vec(),
                    rate_n: event.rate_n.to_string(),
                    rate_d: event.rate_d.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // ConverterAnchorAdded event
            if let Some(event) = converterregistry::events::ConverterAnchorAdded::match_and_decode(log) {
                total_converter_anchor_added += 1;
                let event = pb::log::Log::from(pb::ConverterAnchorAdded { anchor: event.anchor.to_vec() });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // ConverterAnchorRemoved event
            if let Some(event) = converterregistry::events::ConverterAnchorRemoved::match_and_decode(log) {
                total_converter_anchor_removed += 1;
                let event = pb::log::Log::from(pb::ConverterAnchorRemoved { anchor: event.anchor.to_vec() });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // ConvertibleTokenAdded event
            if let Some(event) = converterregistry::events::ConvertibleTokenAdded::match_and_decode(log) {
                total_convertible_token_added += 1;
                let event = pb::log::Log::from(pb::ConvertibleTokenAdded {
                    convertible_token: event.convertible_token.to_vec(),
                    smart_token: event.smart_token.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // ConvertibleTokenRemoved event
            if let Some(event) = converterregistry::events::ConvertibleTokenRemoved::match_and_decode(log) {
                total_convertible_token_removed += 1;
                let event = pb::log::Log::from(pb::ConvertibleTokenRemoved {
                    convertible_token: event.convertible_token.to_vec(),
                    smart_token: event.smart_token.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // LiquidityPoolAdded event
            if let Some(event) = converterregistry::events::LiquidityPoolAdded::match_and_decode(log) {
                total_liquidity_pool_added += 1;
                let event = pb::log::Log::from(pb::LiquidityPoolAdded {
                    liquidity_pool: event.liquidity_pool.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // LiquidityPoolRemoved event
            if let Some(event) = converterregistry::events::LiquidityPoolRemoved::match_and_decode(log) {
                total_liquidity_pool_removed += 1;
                let event = pb::log::Log::from(pb::LiquidityPoolRemoved {
                    liquidity_pool: event.liquidity_pool.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // SmartTokenAdded event
            if let Some(event) = converterregistry::events::SmartTokenAdded::match_and_decode(log) {
                total_smart_token_added += 1;
                let event = pb::log::Log::from(pb::SmartTokenAdded {
                    smart_token: event.smart_token.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // SmartTokenRemoved event
            if let Some(event) = converterregistry::events::SmartTokenRemoved::match_and_decode(log) {
                total_smart_token_removed += 1;
                let event = pb::log::Log::from(pb::SmartTokenRemoved {
                    smart_token: event.smart_token.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // NewConverter event
            if let Some(event) = converterfactory::events::NewConverter::match_and_decode(log) {
                total_new_converter += 1;
                let event = pb::log::Log::from(pb::NewConverter {
                    converter_type: bigint_to_u64(&event.converter_type).unwrap_or_default() as u32,
                    converter: event.converter.to_vec(),
                    owner: event.owner.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // NewConverter event from BancorConverterFactory (Legacy)
            if let Some(event) = bancorconverterfactory::events::NewConverter::match_and_decode(log) {
                total_new_converter_legacy += 1;
                let event = pb::log::Log::from(pb::NewConverter {
                    converter_type: 1,
                    converter: event.converter.to_vec(),
                    owner: event.owner.to_vec(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // FeaturesAddition
            if let Some(event) = contractfeatures::events::FeaturesAddition::match_and_decode(log) {
                total_features_addition += 1;
                let event = pb::log::Log::from(pb::FeaturesAddition {
                    address: event.address.to_vec(),
                    features: event.features.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // FeaturesRemoval
            if let Some(event) = contractfeatures::events::FeaturesRemoval::match_and_decode(log) {
                total_features_removal += 1;
                let event = pb::log::Log::from(pb::FeaturesRemoval {
                    address: event.address.to_vec(),
                    features: event.features.to_string(),
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
    substreams::log::info!("Total Activation events: {}", total_activations);
    substreams::log::info!("Total Conversion events: {}", total_conversions);
    substreams::log::info!("Total ConversionFeeUpdate events: {}", total_conversion_fee_updates);
    substreams::log::info!("Total LiquidityAdded events: {}", total_liquidity_added);
    substreams::log::info!("Total LiquidityRemoved events: {}", total_liquidity_removed);
    substreams::log::info!("Total OwnerUpdate events: {}", total_owner_updates);
    substreams::log::info!("Total TokenRateUpdate events: {}", total_token_rate_updates);
    substreams::log::info!("Total ConverterAnchorAdded events: {}", total_converter_anchor_added);
    substreams::log::info!("Total ConverterAnchorRemoved events: {}", total_converter_anchor_removed);
    substreams::log::info!("Total ConvertibleTokenAdded events: {}", total_convertible_token_added);
    substreams::log::info!("Total ConvertibleTokenRemoved events: {}", total_convertible_token_removed);
    substreams::log::info!("Total LiquidityPoolAdded events: {}", total_liquidity_pool_added);
    substreams::log::info!("Total LiquidityPoolRemoved events: {}", total_liquidity_pool_removed);
    substreams::log::info!("Total SmartTokenAdded events: {}", total_smart_token_added);
    substreams::log::info!("Total SmartTokenRemoved events: {}", total_smart_token_removed);
    substreams::log::info!("Total NewConverter events: {}", total_new_converter);
    substreams::log::info!("Total NewConverter (Legacy) events: {}", total_new_converter_legacy);
    substreams::log::info!("Total FeaturesAddition events: {}", total_features_addition);
    substreams::log::info!("Total FeaturesRemoval events: {}", total_features_removal);
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
