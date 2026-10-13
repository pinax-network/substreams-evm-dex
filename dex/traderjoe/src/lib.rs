//! Trader Joe V2 Liquidity Book pair and factory events, as
//! `traderjoe.v1.Events`.
//!
//! Ported from substreams-evm `dex/traderjoe` (tag `evm-dex-v0.8.0`,
//! `9663b25`); the package README lists what changed and the v0.8.0
//! behaviour it keeps, such as which half of a packed amount is X.
//! [`project`] runs the decoding without the Substreams runtime, for a host
//! program that decodes blocks itself; the `map_events` handler exists only
//! in the WASM build.

use common::create::{CreateLog, CreateTransaction};
use common::try_to_u64;
/// The `traderjoe.v1` messages [`project`] returns.
pub use proto::pb::traderjoe::v1 as pb;
use substreams::errors::Error;
use substreams::scalar::BigInt;
use substreams_abis::dex::traderjoe;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log};
use substreams_ethereum::Event;

/// Decode a packed bytes32 into two uint128 values (upper = x, lower = y).
/// Trader Joe LB uses this encoding: `uint128(x) << 128 | uint128(y)`
///
/// Kept from v0.8.0: Liquidity Book packs X in the low 128 bits, so this
/// probably swaps X and Y (see the README).
fn decode_packed_uint128(bytes: &[u8; 32]) -> (String, String) {
    let x = BigInt::from_unsigned_bytes_be(&bytes[..16]);
    let y = BigInt::from_unsigned_bytes_be(&bytes[16..]);
    (x.to_string(), y.to_string())
}

/// Decodes the Trader Joe V2 events of `block`'s successful transactions,
/// from any emitter: pair `Swap`, `DepositedToBins`, `WithdrawnFromBins` and
/// `CompositionFees`, and factory `LBPairCreated`. Transactions without one
/// are left out.
///
/// # Errors
///
/// Where v0.8.0 panicked, failing the block: a successful transaction
/// without a receipt, or a bin id, volatility accumulator, bin step or pid
/// above `u64::MAX`.
pub fn project(block: &Block) -> Result<pb::Events, Error> {
    // v0.8.0 read every successful transaction's receipt, panicking when one
    // was missing; fail before decoding instead.
    if block.transactions().any(|trx| trx.receipt.is_unset()) {
        return Err(Error::msg("successful transaction has no receipt"));
    }

    let mut events = pb::Events::default();
    let mut total_swaps = 0;
    let mut total_deposits = 0;
    let mut total_withdrawals = 0;
    let mut total_composition_fees = 0;
    let mut total_lb_pair_created = 0;

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        let logs_with_calls: Vec<(&Log, Option<&Call>)> = if trx.calls.is_empty() {
            trx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
        } else {
            trx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
        };
        for (log, call) in logs_with_calls {
            // Swap event (LBPair)
            if let Some(event) = traderjoe::lbpair::events::Swap::match_and_decode(log) {
                total_swaps += 1;
                let (amount_in_x, amount_in_y) = decode_packed_uint128(&event.amounts_in);
                let (amount_out_x, amount_out_y) = decode_packed_uint128(&event.amounts_out);
                let (total_fees_x, total_fees_y) = decode_packed_uint128(&event.total_fees);
                let (protocol_fees_x, protocol_fees_y) = decode_packed_uint128(&event.protocol_fees);
                let event = pb::log::Log::from(pb::Swap {
                    sender: event.sender.to_vec(),
                    to: event.to.to_vec(),
                    id: try_to_u64(&event.id, "Swap.id")? as u32,
                    amount_in_x,
                    amount_in_y,
                    amount_out_x,
                    amount_out_y,
                    volatility_accumulator: try_to_u64(&event.volatility_accumulator, "Swap.volatilityAccumulator")? as u32,
                    total_fees_x,
                    total_fees_y,
                    protocol_fees_x,
                    protocol_fees_y,
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // DepositedToBins event (LBPair)
            if let Some(event) = traderjoe::lbpair::events::DepositedToBins::match_and_decode(log) {
                total_deposits += 1;
                let event = pb::log::Log::from(pb::DepositedToBins {
                    sender: event.sender.to_vec(),
                    to: event.to.to_vec(),
                    ids: event.ids.iter().map(|id| try_to_u64(id, "DepositedToBins.ids")).collect::<Result<_, _>>()?,
                    amounts: event.amounts.iter().map(|a| a.to_vec()).collect(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // WithdrawnFromBins event (LBPair)
            if let Some(event) = traderjoe::lbpair::events::WithdrawnFromBins::match_and_decode(log) {
                total_withdrawals += 1;
                let event = pb::log::Log::from(pb::WithdrawnFromBins {
                    sender: event.sender.to_vec(),
                    to: event.to.to_vec(),
                    ids: event.ids.iter().map(|id| try_to_u64(id, "WithdrawnFromBins.ids")).collect::<Result<_, _>>()?,
                    amounts: event.amounts.iter().map(|a| a.to_vec()).collect(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // CompositionFees event (LBPair)
            if let Some(event) = traderjoe::lbpair::events::CompositionFees::match_and_decode(log) {
                total_composition_fees += 1;
                let (total_fees_x, total_fees_y) = decode_packed_uint128(&event.total_fees);
                let (protocol_fees_x, protocol_fees_y) = decode_packed_uint128(&event.protocol_fees);
                let event = pb::log::Log::from(pb::CompositionFees {
                    sender: event.sender.to_vec(),
                    id: try_to_u64(&event.id, "CompositionFees.id")? as u32,
                    total_fees_x,
                    total_fees_y,
                    protocol_fees_x,
                    protocol_fees_y,
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // LbPairCreated event (LBFactory)
            if let Some(event) = traderjoe::lbfactory::events::LbPairCreated::match_and_decode(log) {
                total_lb_pair_created += 1;
                let event = pb::log::Log::from(pb::LbPairCreated {
                    token_x: event.token_x.to_vec(),
                    token_y: event.token_y.to_vec(),
                    bin_step: try_to_u64(&event.bin_step, "LBPairCreated.binStep")? as u32,
                    lb_pair: event.lb_pair.to_vec(),
                    pid: try_to_u64(&event.pid, "LBPairCreated.pid")? as u32,
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
    substreams::log::info!("Total DepositedToBins events: {}", total_deposits);
    substreams::log::info!("Total WithdrawnFromBins events: {}", total_withdrawals);
    substreams::log::info!("Total CompositionFees events: {}", total_composition_fees);
    substreams::log::info!("Total LbPairCreated events: {}", total_lb_pair_created);
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
