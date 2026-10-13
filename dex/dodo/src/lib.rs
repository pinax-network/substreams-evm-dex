//! DODO V2 RouteProxy swaps (`OrderHistory`), as `dodo.v1.Events`.
//!
//! Ported from substreams-evm `dex/dodo` (tag `evm-dex-v0.8.0`, `9663b25`);
//! the package README lists what changed. [`project`] runs the decoding
//! without the Substreams runtime, for a host program that decodes blocks
//! itself; the `map_events` handler exists only in the WASM build.

use common::create::{CreateLog, CreateTransaction};
/// The `dodo.v1` messages [`project`] returns.
pub use proto::pb::dodo::v1 as pb;
use substreams::errors::Error;
use substreams_abis::dex::dodo;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log};
use substreams_ethereum::Event;

/// Decodes the DODO V2 RouteProxy `OrderHistory` events of `block`'s
/// successful transactions, from any emitter. Transactions without one are
/// left out.
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
    let mut total_order_history = 0;

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        let logs_with_calls: Vec<(&Log, Option<&Call>)> = if trx.calls.is_empty() {
            trx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
        } else {
            trx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
        };
        for (log, call) in logs_with_calls {
            // OrderHistory event (DODO swap)
            if let Some(event) = dodo::v2::routeproxy::events::OrderHistory::match_and_decode(log) {
                total_order_history += 1;
                let event = pb::log::Log::from(pb::OrderHistory {
                    from_token: event.from_token.to_vec(),
                    to_token: event.to_token.to_vec(),
                    sender: event.sender.to_vec(),
                    from_amount: event.from_amount.to_string(),
                    return_amount: event.return_amount.to_string(),
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
    substreams::log::info!("Total OrderHistory events: {}", total_order_history);
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
mod tests {
    //! One synthetic log per decoded event; topic 0 is the Keccak-256 of the
    //! canonical signature from the substreams-abis ABI JSON.

    use super::*;
    use common::testing::*;

    fn order_history_log() -> Log {
        log(
            &addr(0x11),
            vec![topic0("OrderHistory(address,address,address,uint256,uint256)")],
            [word(&addr(0xa1)), word(&addr(0xa2)), word(&addr(0xa3)), uint(1_000), uint(999)].concat(),
        )
    }

    common::transaction_tests!(order_history_log());

    #[test]
    fn order_history() {
        let events = project(&block(vec![trx(&addr(0x01), vec![order_history_log()])])).expect("project");
        assert_eq!(
            events.transactions[0].logs[0].log,
            Some(pb::log::Log::from(pb::OrderHistory {
                from_token: addr(0xa1),
                to_token: addr(0xa2),
                sender: addr(0xa3),
                from_amount: "1000".into(),
                return_amount: "999".into(),
            }))
        );
    }
}
