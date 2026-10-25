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

use buffa::Message;
use columns::{set_call, set_log, set_transaction, BlockColumns};
pub use encoding::Encoding;
/// The `dex.swaps.v1` messages [`changes`] takes.
pub use proto::pb::dex::swaps::v1 as pb;
use substreams::{errors::Error, pb::substreams::Clock};
use substreams_database_change::{
    pb::sf::substreams::sink::database::v1::DatabaseChanges,
    tables::{Row, Tables},
};

/// The largest `max_logs` that [`changes_bounded`] accepts.
pub const MAX_LOGS_BOUND: usize = 100_000;
/// The largest `max_bytes` that [`changes_bounded`] accepts: 64 MiB.
pub const MAX_BYTES_BOUND: usize = 64 * 1024 * 1024;

/// The `db_out` body: one row per event of `events`, in order, then the
/// `blocks` row.
///
/// # Errors
///
/// A clock without a timestamp, when `events` holds an event (v0.8.0
/// panicked). Practically unreachable: a row whose encoded size exceeds
/// buffa's 2 GiB message limit, or a total encoded size that overflows
/// `usize`.
pub fn changes(clock: &Clock, events: &pb::Events, encoding: Encoding) -> Result<DatabaseChanges, Error> {
    changes_with_limit(clock, events, encoding, None)
}

/// Format native rows within explicit input, log-count and output bounds.
///
/// Each row is checked before it joins the retained batch, so transaction and
/// clock fields cannot multiply across the full batch before rejection. At most
/// one candidate row is materialized outside the output budget. Hex expansion
/// and the block hash, repeated in its key and fields, can make that row up to
/// about twice the encoded input, so memory stays linear in the input. No
/// partial result is returned on failure. Row contents, keys, operations and
/// ordinals are identical to [`changes`].
///
/// `max_bytes` bounds encoded bytes, not memory: retained rows hold about 4x their encoded size, and a rejected
/// block peaks at up to about 9x `max_bytes` for logs of at most four 32-byte topics, as every EVM and TRON log has.
///
/// # Errors
///
/// - `max_logs` outside `1..=`[`MAX_LOGS_BOUND`] or `max_bytes` outside
///   `1..=`[`MAX_BYTES_BOUND`];
/// - more than `max_logs` logs, counting entries without an event;
/// - an encoded input plus clock above `max_bytes`;
/// - an encoded output above `max_bytes`, checked row by row;
/// - and the errors of [`changes`] for a block that writes rows.
pub fn changes_bounded(clock: &Clock, events: &pb::Events, encoding: Encoding, max_logs: usize, max_bytes: usize) -> Result<DatabaseChanges, Error> {
    if !(1..=MAX_LOGS_BOUND).contains(&max_logs) || !(1..=MAX_BYTES_BOUND).contains(&max_bytes) {
        return Err(Error::msg("invalid row formatting bounds"));
    }
    let count = events.transactions.iter().try_fold(0usize, |sum, tx| sum.checked_add(tx.logs.len()));
    if count.is_none_or(|count| count > max_logs) {
        return Err(Error::msg("row formatting log capacity exceeded"));
    }
    if (events.try_encoded_len()? as usize).saturating_add(clock.try_encoded_len()? as usize) > max_bytes {
        return Err(Error::msg("row formatting input capacity exceeded"));
    }
    changes_with_limit(clock, events, encoding, Some(max_bytes))
}

fn changes_with_limit(clock: &Clock, events: &pb::Events, encoding: Encoding, max_bytes: Option<usize>) -> Result<DatabaseChanges, Error> {
    // v0.8.0 reads the clock's timestamp only to write a row.
    let has_events = events.transactions.iter().flat_map(|tx| &tx.logs).any(|log| log.log.is_some());
    if !has_events {
        return Ok(DatabaseChanges::default());
    }
    let block = BlockColumns::new(clock)?;
    let mut changes = DatabaseChanges::default();
    let mut encoded_bytes = 0usize;
    let mut append = |tables: Tables| -> Result<(), Error> {
        let mut candidate = tables.to_database_changes();
        debug_assert_eq!(candidate.table_changes.len(), 1);
        candidate.table_changes[0].ordinal = changes.table_changes.len() as u64;
        // Include the repeated-message tag/length and the actual global ordinal.
        encoded_bytes = encoded_bytes
            .checked_add(candidate.try_encoded_len()? as usize)
            .ok_or_else(|| Error::msg("row formatting output capacity exceeded"))?;
        if max_bytes.is_some_and(|limit| encoded_bytes > limit) {
            return Err(Error::msg("row formatting output capacity exceeded"));
        }
        changes.table_changes.append(&mut candidate.table_changes);
        Ok(())
    };

    for (tx_index, tx) in events.transactions.iter().enumerate() {
        for (log_index, log) in tx.logs.iter().enumerate() {
            let Some(event) = &log.log else { continue };
            let table = match event {
                pb::log::Log::Swap(_) => "swaps",
                pb::log::Log::SwapFee(_) => "swap_fee",
                pb::log::Log::Initialize(_) => "initialize",
            };
            let mut tables = Tables::new();
            let row = tables.create_row(table, block.log_key(tx_index, log_index));
            block.set(row);
            set_transaction(encoding, tx, tx_index, row);
            set_log(encoding, log, log_index, row);
            set_call(encoding, log.call.as_option(), row);
            set_event(encoding, event, row);
            append(tables)?;
        }
    }

    // ONLY include blocks if events are present (as v0.8.0).
    let mut tables = Tables::new();
    block.set(tables.create_row("blocks", [("block_num", block.block_num.clone())]));
    append(tables)?;

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
