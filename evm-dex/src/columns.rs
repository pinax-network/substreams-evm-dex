//! The block, transaction, log and call columns every row carries. Ported
//! from substreams-evm `common/src/clickhouse.rs` (`9663b25`): `log_key`,
//! `set_clock` and the `set_template_tx`, `set_template_log` and
//! `set_template_call` templates with their `dex.swaps.v1` trait
//! implementations, which read a missing `Call` as all-default fields.

use buffa::Enumeration;
use substreams::{errors::Error, pb::substreams::Clock};
use substreams_database_change::tables::Row;

use crate::encoding::{hex, Encoding};
use crate::pb;

/// The clock columns (v0.8.0's `set_clock`), and the key of the log tables.
pub(crate) struct BlockColumns {
    pub(crate) block_num: String,
    block_hash: String,
    timestamp: String,
    minute: String,
}

impl BlockColumns {
    /// # Errors
    ///
    /// A clock without a timestamp, where v0.8.0 panicked.
    pub(crate) fn new(clock: &Clock) -> Result<Self, Error> {
        let seconds = clock.timestamp.as_option().ok_or_else(|| Error::msg("clock.timestamp is required"))?.seconds;
        Ok(Self {
            block_num: clock.number.to_string(),
            block_hash: format!("0x{}", clock.id),
            timestamp: seconds.to_string(),
            minute: (seconds / 60).to_string(),
        })
    }

    /// The key of a `swaps`, `swap_fee` or `initialize` row (v0.8.0's
    /// `log_key`). `tx_index` and `log_index` are positions in
    /// `Events.transactions` and `Transaction.logs`.
    pub(crate) fn log_key(&self, tx_index: usize, log_index: usize) -> [(&'static str, String); 6] {
        [
            ("minute", self.minute.clone()),
            ("timestamp", self.timestamp.clone()),
            ("block_num", self.block_num.clone()),
            ("tx_index", tx_index.to_string()),
            ("log_index", log_index.to_string()),
            ("block_hash", self.block_hash.clone()),
        ]
    }

    /// `block_num`, `block_hash` (`0x` + clock id), `timestamp` (seconds) and
    /// `minute` (seconds / 60).
    pub(crate) fn set(&self, row: &mut Row) {
        row.set("block_num", &self.block_num)
            .set("block_hash", &self.block_hash)
            .set("timestamp", &self.timestamp)
            .set("minute", &self.minute);
    }
}

/// The `tx_*` columns (v0.8.0's `set_template_tx`). `tx_index` is the
/// position in `Events.transactions`; `tx_hash` is hex in either encoding and
/// a contract creation has an empty `tx_to`.
pub(crate) fn set_transaction(encoding: Encoding, tx: &pb::Transaction, tx_index: usize, row: &mut Row) {
    let tx_to = tx.to.as_deref().map(|to| encoding.address(to)).unwrap_or_default();
    row.set("tx_index", tx_index as u32)
        .set("tx_hash", hex(&tx.hash))
        .set("tx_from", encoding.address(&tx.from))
        .set("tx_to", tx_to)
        .set("tx_nonce", tx.nonce)
        .set("tx_gas_price", tx.gas_price.as_str())
        .set("tx_gas_limit", tx.gas_limit)
        .set("tx_gas_used", tx.gas_used)
        .set("tx_value", tx.value.as_str());
}

/// The `log_*` columns (v0.8.0's `set_template_log`). `log_index` is the
/// position in `Transaction.logs`; topics (comma-separated) and data are hex
/// in either encoding.
pub(crate) fn set_log(encoding: Encoding, log: &pb::Log, log_index: usize, row: &mut Row) {
    let topics: Vec<String> = log.topics.iter().map(|topic| hex(topic)).collect();
    row.set("log_index", log_index as u32)
        .set("log_block_index", log.block_index)
        .set("log_address", encoding.address(&log.address))
        .set("log_ordinal", log.ordinal)
        .set("log_topics", topics.join(","))
        .set("log_data", hex(&log.data));
}

/// The `call_*` columns (v0.8.0's `set_template_call`). Without call
/// metadata every column takes its default: the address of no bytes (`0x`,
/// or empty in Tron Base58), 0, an empty `call_value` and
/// `CALL_TYPE_UNSPECIFIED`, as does a call type outside the enum.
pub(crate) fn set_call(encoding: Encoding, call: Option<&pb::Call>, row: &mut Row) {
    let call_type = call.and_then(|c| c.call_type.as_known()).unwrap_or_default();
    row.set("call_caller", encoding.address(call.map(|c| c.caller.as_slice()).unwrap_or_default()))
        .set("call_index", call.map(|c| c.index).unwrap_or_default())
        .set("call_begin_ordinal", call.map(|c| c.begin_ordinal).unwrap_or_default())
        .set("call_end_ordinal", call.map(|c| c.end_ordinal).unwrap_or_default())
        .set("call_address", encoding.address(call.map(|c| c.address.as_slice()).unwrap_or_default()))
        .set("call_value", call.map(|c| c.value.as_str()).unwrap_or_default())
        .set("call_gas_consumed", call.map(|c| c.gas_consumed).unwrap_or_default())
        .set("call_gas_limit", call.map(|c| c.gas_limit).unwrap_or_default())
        .set("call_depth", call.map(|c| c.depth).unwrap_or_default())
        .set("call_parent_index", call.map(|c| c.parent_index).unwrap_or_default())
        .set("call_type", call_type.proto_name());
}
