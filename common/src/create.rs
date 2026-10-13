//! Templates that copy transaction, log and call metadata from a Firehose
//! block into `dex.swaps.v1` messages.
//!
//! Ported from substreams-evm `common/src/create.rs` (`9663b25`), where one
//! macro implemented them for every per-protocol package; only the
//! `dex.swaps.v1` implementation is kept, and the unused synthetic-log
//! template is dropped. Field values are those of v0.8.0.

use proto::pb::dex::swaps::v1 as pb;
use substreams_ethereum::pb::eth::v2::{BigInt, Call, Log, TransactionTrace};

/// Creates call metadata from a raw EVM call.
pub trait CreateCall {
    fn create_call(call: &Call) -> Self;
}

/// Creates a log entry from a raw log and the event decoded from it.
pub trait CreateLog<E> {
    fn create_log(log: &Log, event: E) -> Self;
    fn create_log_with_call(log: &Log, event: E, call: Option<&Call>) -> Self;
}

/// Creates a transaction from a transaction trace.
pub trait CreateTransaction {
    fn create_transaction(trx: &TransactionTrace) -> Self;
}

/// A Firehose integer as a decimal string; an unset value is `"0"`.
fn decimal(value: Option<&BigInt>) -> String {
    value.cloned().unwrap_or_default().with_decimal(0).to_string()
}

impl CreateCall for pb::Call {
    fn create_call(call: &Call) -> Self {
        Self {
            index: call.index,
            begin_ordinal: call.begin_ordinal,
            end_ordinal: call.end_ordinal,
            caller: call.caller.to_vec(),
            address: call.address.to_vec(),
            value: decimal(call.value.as_option()),
            gas_consumed: call.gas_consumed,
            gas_limit: call.gas_limit,
            depth: call.depth,
            parent_index: call.parent_index,
            // Both enums number their variants alike; an unknown value is
            // carried through unchanged, as prost's raw i32 was.
            call_type: call.call_type.to_i32().into(),
        }
    }
}

impl CreateLog<pb::log::Log> for pb::Log {
    fn create_log(log: &Log, event: pb::log::Log) -> Self {
        Self::create_log_with_call(log, event, None)
    }

    fn create_log_with_call(log: &Log, event: pb::log::Log, call: Option<&Call>) -> Self {
        Self {
            address: log.address.to_vec(),
            ordinal: log.ordinal,
            topics: log.topics.iter().map(|t| t.to_vec()).collect(),
            data: log.data.to_vec(),
            call: call.map(pb::Call::create_call).into(),
            block_index: log.block_index,
            log: Some(event),
        }
    }
}

impl CreateTransaction for pb::Transaction {
    /// # Panics
    ///
    /// Panics if the transaction carries no receipt, as v0.8.0 did.
    /// `dex_swaps::project` rejects such blocks before it calls this.
    fn create_transaction(trx: &TransactionTrace) -> Self {
        Self {
            from: trx.from.to_vec(),
            to: if trx.to.is_empty() { None } else { Some(trx.to.to_vec()) },
            hash: trx.hash.to_vec(),
            nonce: trx.nonce,
            gas_price: decimal(trx.gas_price.as_option()),
            gas_limit: trx.gas_limit,
            // v0.8.0 reports the receipt's cumulative gas, not the
            // transaction's own `gas_used`; kept for parity.
            gas_used: trx.receipt().receipt.cumulative_gas_used,
            value: decimal(trx.value.as_option()),
            logs: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    //! Ported from substreams-evm `common/tests/create_log_tests.rs`
    //! (`9663b25`), against `dex.swaps.v1` instead of the ERC-20 transfer
    //! messages.
    use super::*;
    use buffa::MessageField;
    use substreams_ethereum::pb::eth::v2::{CallType, TransactionReceipt};

    fn sample_call() -> Call {
        Call {
            index: 7,
            parent_index: 3,
            depth: 2,
            call_type: CallType::Delegate.into(),
            caller: vec![0x22; 20],
            address: vec![0x33; 20],
            value: MessageField::some(BigInt { bytes: vec![0x2a] }),
            gas_limit: 99,
            gas_consumed: 77,
            begin_ordinal: 100,
            end_ordinal: 200,
            input: vec![0xde, 0xad, 0xbe, 0xef],
            ..Default::default()
        }
    }

    fn sample_transaction() -> TransactionTrace {
        TransactionTrace {
            to: vec![0x77; 20],
            nonce: 9,
            gas_price: MessageField::some(BigInt { bytes: vec![0x64] }),
            gas_limit: 500,
            value: MessageField::some(BigInt { bytes: vec![0x2a] }),
            input: vec![0xca, 0xfe],
            gas_used: 321,
            hash: vec![0x88; 32],
            from: vec![0x99; 20],
            receipt: MessageField::some(TransactionReceipt {
                cumulative_gas_used: 654,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn sample_event() -> pb::log::Log {
        pb::log::Log::from(pb::Initialize {
            protocol: pb::Protocol::UniswapV2.into(),
            factory: vec![0x44; 20],
            pool: vec![0x55; 20],
        })
    }

    #[test]
    fn create_call_maps_rich_call_metadata() {
        let created = pb::Call::create_call(&sample_call());

        assert_eq!(created.index, 7);
        assert_eq!(created.parent_index, 3);
        assert_eq!(created.begin_ordinal, 100);
        assert_eq!(created.end_ordinal, 200);
        assert_eq!(created.caller, vec![0x22; 20]);
        assert_eq!(created.address, vec![0x33; 20]);
        assert_eq!(created.value, "42");
        assert_eq!(created.gas_limit, 99);
        assert_eq!(created.gas_consumed, 77);
        assert_eq!(created.depth, 2);
        assert_eq!(created.call_type, pb::CallType::Delegate);
    }

    #[test]
    fn create_call_keeps_unset_values_and_unknown_call_types() {
        let mut call = sample_call();
        call.value = MessageField::none();
        call.call_type = 42.into();

        let created = pb::Call::create_call(&call);

        assert_eq!(created.value, "0");
        assert_eq!(created.call_type.to_i32(), 42);
    }

    #[test]
    fn create_transaction_populates_metadata_fields() {
        let created = pb::Transaction::create_transaction(&sample_transaction());

        assert_eq!(created.hash, vec![0x88; 32]);
        assert_eq!(created.from, vec![0x99; 20]);
        assert_eq!(created.to, Some(vec![0x77; 20]));
        assert_eq!(created.nonce, 9);
        assert_eq!(created.gas_price, "100");
        assert_eq!(created.gas_limit, 500);
        // v0.8.0 parity: the receipt's cumulative gas, not `gas_used`.
        assert_eq!(created.gas_used, 654);
        assert_eq!(created.value, "42");
        assert!(created.logs.is_empty());
    }

    #[test]
    fn create_transaction_maps_contract_creation_and_unset_amounts() {
        let mut trx = sample_transaction();
        trx.to.clear();
        trx.gas_price = MessageField::none();
        trx.value = MessageField::none();

        let created = pb::Transaction::create_transaction(&trx);

        assert_eq!(created.to, None);
        assert_eq!(created.gas_price, "0");
        assert_eq!(created.value, "0");
    }

    #[test]
    fn create_transaction_writes_large_amounts_as_integers() {
        let mut trx = sample_transaction();
        // 2^128, wider than any machine integer.
        let mut bytes = vec![1];
        bytes.extend([0; 16]);
        trx.value = MessageField::some(BigInt { bytes });

        let created = pb::Transaction::create_transaction(&trx);

        assert_eq!(created.value, "340282366920938463463374607431768211456");
    }

    #[test]
    fn create_log_with_call_maps_native_positions_and_rich_call_metadata() {
        let log = Log {
            address: vec![0x11; 20],
            topics: vec![vec![0xaa; 32], vec![0xbb; 32]],
            data: vec![0xcc, 0xdd],
            index: 4,
            block_index: 12,
            ordinal: 42,
        };
        let call = sample_call();

        let created = pb::Log::create_log_with_call(&log, sample_event(), Some(&call));

        assert_eq!(created.address, vec![0x11; 20]);
        assert_eq!(created.topics, vec![vec![0xaa; 32], vec![0xbb; 32]]);
        assert_eq!(created.data, vec![0xcc, 0xdd]);
        assert_eq!(created.block_index, 12);
        assert_eq!(created.ordinal, 42);
        assert_eq!(created.log, Some(sample_event()));

        let created_call = created.call.as_option().expect("call metadata should be populated");
        assert_eq!(created_call.index, 7);
        assert_eq!(created_call.parent_index, 3);
        assert_eq!(created_call.begin_ordinal, 100);
        assert_eq!(created_call.end_ordinal, 200);
        assert_eq!(created_call.caller, vec![0x22; 20]);
        assert_eq!(created_call.address, vec![0x33; 20]);
        assert_eq!(created_call.value, "42");
        assert_eq!(created_call.gas_limit, 99);
        assert_eq!(created_call.gas_consumed, 77);
        assert_eq!(created_call.call_type, pb::CallType::Delegate);
    }

    #[test]
    fn create_log_without_call_leaves_call_unset() {
        let created = pb::Log::create_log(&Log::default(), sample_event());

        assert!(created.call.is_unset());
        assert_eq!(created.log, Some(sample_event()));
    }
}
