//! Builders for synthetic Firehose blocks and ABI-encoded logs. Topic 0 is
//! the Keccak-256 of the canonical event signature, so a test also proves
//! that the `substreams-abis` decoder matches that signature.

use substreams_ethereum::pb::eth::v2::{Block, Call, CallType, Log, TransactionReceipt, TransactionTrace, TransactionTraceStatus};
use tiny_keccak::{Hasher, Keccak};

pub fn topic0(signature: &str) -> Vec<u8> {
    let mut hasher = Keccak::v256();
    hasher.update(signature.as_bytes());
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    out.to_vec()
}

pub fn addr(byte: u8) -> Vec<u8> {
    vec![byte; 20]
}

/// An address, or any value up to 32 bytes, left-padded to a word.
pub fn word(value: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 32 - value.len()];
    out.extend_from_slice(value);
    out
}

pub fn uint(value: u128) -> Vec<u8> {
    word(&value.to_be_bytes())
}

pub fn concat(words: &[Vec<u8>]) -> Vec<u8> {
    words.concat()
}

pub fn log(address: &[u8], topics: Vec<Vec<u8>>, data: Vec<u8>) -> Log {
    Log {
        address: address.to_vec(),
        topics,
        data,
        ..Default::default()
    }
}

/// A successful transaction whose root call (a CALL to `to`) emitted `logs`;
/// the receipt carries the same logs. Log ordinals follow the given order.
pub fn trx(to: &[u8], logs: Vec<Log>) -> TransactionTrace {
    let logs: Vec<Log> = logs
        .into_iter()
        .enumerate()
        .map(|(i, mut log)| {
            log.ordinal = 10 + i as u64;
            log.index = i as u32;
            log
        })
        .collect();
    TransactionTrace {
        status: TransactionTraceStatus::Succeeded.into(),
        from: addr(0xf0),
        to: to.to_vec(),
        hash: vec![0xaa; 32],
        calls: vec![Call {
            index: 1,
            call_type: CallType::Call.into(),
            address: to.to_vec(),
            logs: logs.clone(),
            ..Default::default()
        }],
        receipt: TransactionReceipt { logs, ..Default::default() }.into(),
        ..Default::default()
    }
}

pub fn block(transaction_traces: Vec<TransactionTrace>) -> Block {
    Block {
        number: 1,
        transaction_traces,
        ..Default::default()
    }
}
