//! One `db_out` that writes the `dex.swaps.v1` events of `dex-swaps` into the
//! Token API DEX ClickHouse tables.
//!
//! Ported from substreams-evm `evm-dex` (tag `evm-dex-v0.8.0`, `9663b25`); the
//! rows are v0.8.0's. Every log with an event becomes one CREATE row:
//! - `swaps` for a `Swap`, `swap_fee` for a `SwapFee`, `initialize` for an
//!   `Initialize`, keyed by `(minute, timestamp, block_num, tx_index,
//!   log_index, block_hash)`;
//! - then one `blocks` row, keyed by `block_num`, when the block wrote any.
//!
//! Every row carries the clock columns; the log rows also carry the
//! transaction, log and call columns of their log. Addresses follow the
//! [`Encoding`] of the `db_out` param (`hex` or `tron_base58`); hashes, topics
//! and data are always hex.
//!
//! Where v0.8.0 panicked, [`changes`] returns an error, and the block fails
//! either way: an unknown param, or a clock without a timestamp in a block
//! that writes rows. The changes are encoded by `substreams-database-change`
//! 5.0.0, where every `Field` also carries `update_op = UPDATE_OP_SET`
//! (field 4); v0.8.0's 3.0.0 has no such field, and the SQL sink uses only a
//! CREATE field's name and value. Fields are sorted by name.
mod columns;
mod encoding;

use columns::{set_call, set_log, set_transaction, BlockColumns};
pub use encoding::Encoding;
/// The `dex.swaps.v1` messages [`changes`] takes.
pub use proto::pb::dex::swaps::v1 as pb;
use substreams::{errors::Error, pb::substreams::Clock};
use substreams_database_change::{
    pb::sf::substreams::sink::database::v1::DatabaseChanges,
    tables::{Row, Tables},
};

/// The `db_out` body: one row per event of `events`, in order, then the
/// `blocks` row.
///
/// # Errors
///
/// A clock without a timestamp, when `events` holds an event (v0.8.0
/// panicked).
pub fn changes(clock: &Clock, events: &pb::Events, encoding: Encoding) -> Result<DatabaseChanges, Error> {
    // v0.8.0 reads the clock's timestamp only to write a row.
    let has_events = events.transactions.iter().flat_map(|tx| &tx.logs).any(|log| log.log.is_some());
    if !has_events {
        return Ok(DatabaseChanges::default());
    }
    let block = BlockColumns::new(clock)?;
    let mut tables = Tables::new();

    for (tx_index, tx) in events.transactions.iter().enumerate() {
        for (log_index, log) in tx.logs.iter().enumerate() {
            let Some(event) = &log.log else { continue };
            let table = match event {
                pb::log::Log::Swap(_) => "swaps",
                pb::log::Log::SwapFee(_) => "swap_fee",
                pb::log::Log::Initialize(_) => "initialize",
            };
            let row = tables.create_row(table, block.log_key(tx_index, log_index));
            block.set(row);
            set_transaction(encoding, tx, tx_index, row);
            set_log(encoding, log, log_index, row);
            set_call(encoding, log.call.as_option(), row);
            set_event(encoding, event, row);
        }
    }

    // ONLY include blocks if events are present (as v0.8.0).
    block.set(tables.create_row("blocks", [("block_num", block.block_num.clone())]));

    let mut changes = tables.to_database_changes();
    // The crate emits a row's fields in `HashMap` order, which changes with
    // every WASM build; the sink reads them by name. Sorting keeps the output
    // bytes stable across builds. Composite keys stay in map order.
    for change in &mut changes.table_changes {
        change.fields.sort_by(|a, b| a.name.cmp(&b.name));
    }
    Ok(changes)
}

/// The columns of the event itself.
fn set_event(encoding: Encoding, event: &pb::log::Log, row: &mut Row) {
    match event {
        pb::log::Log::Swap(swap) => {
            row.set("protocol", protocol_name(swap.protocol))
                .set("factory", encoding.address(&swap.factory))
                .set("pool", encoding.address(&swap.pool))
                .set("user", encoding.address(&swap.user))
                .set("input_contract", encoding.address(&swap.input_token))
                .set("input_amount", swap.input_amount.as_str())
                .set("output_contract", encoding.address(&swap.output_token))
                .set("output_amount", swap.output_amount.as_str());
        }
        pb::log::Log::SwapFee(swap_fee) => {
            row.set("protocol", protocol_name(swap_fee.protocol))
                .set("factory", encoding.address(&swap_fee.factory))
                .set("pool", encoding.address(&swap_fee.pool))
                .set("fee", swap_fee.fee);
        }
        pb::log::Log::Initialize(initialize) => {
            row.set("protocol", protocol_name(initialize.protocol))
                .set("factory", encoding.address(&initialize.factory))
                .set("pool", encoding.address(&initialize.pool));
        }
    }
}

/// The `protocol` column: v0.8.0's names, which the tables' `Enum8` lists,
/// and `unspecified` (not in the `Enum8`) for `PROTOCOL_UNSPECIFIED` or a
/// value outside the enum.
fn protocol_name(protocol: buffa::EnumValue<pb::Protocol>) -> &'static str {
    match protocol.as_known().unwrap_or_default() {
        pb::Protocol::PROTOCOL_AERODROME => "aerodrome",
        pb::Protocol::PROTOCOL_BALANCER => "balancer",
        pb::Protocol::PROTOCOL_BANCOR => "bancor",
        pb::Protocol::PROTOCOL_COW => "cow",
        pb::Protocol::PROTOCOL_CURVEFI => "curvefi",
        pb::Protocol::PROTOCOL_DODO => "dodo",
        pb::Protocol::PROTOCOL_KYBER_ELASTIC => "kyber_elastic",
        pb::Protocol::PROTOCOL_SUNPUMP => "sunpump",
        pb::Protocol::PROTOCOL_TRADERJOE => "traderjoe",
        pb::Protocol::PROTOCOL_UNISWAP_V1 => "uniswap_v1",
        pb::Protocol::PROTOCOL_UNISWAP_V2 => "uniswap_v2",
        pb::Protocol::PROTOCOL_UNISWAP_V3 => "uniswap_v3",
        pb::Protocol::PROTOCOL_UNISWAP_V4 => "uniswap_v4",
        pb::Protocol::PROTOCOL_WOOFI => "woofi",
        pb::Protocol::PROTOCOL_UNSPECIFIED => "unspecified",
    }
}

// The SDK macro generates raw-pointer parameter decoding and discards function
// attributes. As in the map packages, the export exists only in the WASM build.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;

    /// `params` is the address encoding: `hex` (the manifest default) or
    /// `tron_base58`.
    #[substreams::handlers::map]
    fn db_out(params: String, clock: Clock, events: pb::Events) -> Result<DatabaseChanges, Error> {
        let encoding = Encoding::from_param(&params)?;
        let changes = changes(&clock, &events, encoding)?;
        substreams::log::info!("Total rows {}", changes.table_changes.len());
        Ok(changes)
    }
}
