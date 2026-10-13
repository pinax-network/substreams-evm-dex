//! Builders for synthetic Firehose blocks and ABI-encoded logs, for the tests
//! of the per-protocol packages (feature `testing`, enabled only through their
//! dev-dependencies).
//!
//! Topic 0 comes from [`topic0`], the Keccak-256 of the canonical event
//! signature, so a test also proves that the `substreams-abis` decoder matches
//! that signature. [`encode`] writes ABI data independently of the decoders.

/// The Firehose block types the builders return.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::{Block, Call, CallType, Log, TransactionReceipt, TransactionTrace, TransactionTraceStatus};
use tiny_keccak::{Hasher, Keccak};

/// The Keccak-256 of a canonical event signature such as
/// `"Swap(address,uint256,uint256,uint256,uint256,address)"`.
pub fn topic0(signature: &str) -> Vec<u8> {
    let mut hasher = Keccak::v256();
    hasher.update(signature.as_bytes());
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    out.to_vec()
}

/// A 20-byte address of one repeated byte.
pub fn addr(byte: u8) -> Vec<u8> {
    vec![byte; 20]
}

/// An address, or any value up to 32 bytes, left-padded to a word.
pub fn word(value: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 32 - value.len()];
    out.extend_from_slice(value);
    out
}

/// An unsigned integer as a word.
pub fn uint(value: u128) -> Vec<u8> {
    word(&value.to_be_bytes())
}

/// A signed integer, sign-extended to a word as the ABI encodes `intN`.
pub fn int(value: i128) -> Vec<u8> {
    let fill = if value < 0 { 0xff } else { 0 };
    let mut out = vec![fill; 16];
    out.extend_from_slice(&value.to_be_bytes());
    out
}

/// A `bool` as a word.
pub fn boolean(value: bool) -> Vec<u8> {
    uint(value.into())
}

/// A `bytesN` value, right-padded to a word.
pub fn fixed(value: &[u8]) -> Vec<u8> {
    let mut out = value.to_vec();
    out.resize(32, 0);
    out
}

/// One ABI value, for [`encode`].
pub enum Abi {
    /// A static value already encoded as one word.
    Word(Vec<u8>),
    /// `bytes` or `string`.
    Bytes(Vec<u8>),
    /// A dynamic-length array `T[]`.
    Array(Vec<Abi>),
    /// A tuple, or a fixed-length array `T[N]`.
    Tuple(Vec<Abi>),
}

impl Abi {
    fn is_dynamic(&self) -> bool {
        match self {
            Abi::Word(_) => false,
            Abi::Bytes(_) | Abi::Array(_) => true,
            Abi::Tuple(values) => values.iter().any(Abi::is_dynamic),
        }
    }
}

/// The standard ABI encoding of `values`, as event data or as constructor
/// arguments: a head of words and offsets, then the dynamic tails.
pub fn encode(values: &[Abi]) -> Vec<u8> {
    let head_len: usize = values.iter().map(|value| if value.is_dynamic() { 32 } else { static_len(value) }).sum();
    let mut head = Vec::with_capacity(head_len);
    let mut tail = Vec::new();
    for value in values {
        if value.is_dynamic() {
            head.extend(uint((head_len + tail.len()) as u128));
            tail.extend(encode_value(value));
        } else {
            head.extend(encode_value(value));
        }
    }
    head.extend(tail);
    head
}

fn static_len(value: &Abi) -> usize {
    match value {
        Abi::Tuple(values) => values.iter().map(static_len).sum(),
        _ => 32,
    }
}

fn encode_value(value: &Abi) -> Vec<u8> {
    match value {
        Abi::Word(word) => word.clone(),
        Abi::Bytes(bytes) => {
            let mut out = uint(bytes.len() as u128);
            out.extend(bytes);
            out.resize(32 + bytes.len().div_ceil(32) * 32, 0);
            out
        }
        Abi::Array(values) => {
            let mut out = uint(values.len() as u128);
            out.extend(encode(values));
            out
        }
        Abi::Tuple(values) => encode(values),
    }
}

/// A log with no position; [`trx`] numbers it.
pub fn log(address: &[u8], topics: Vec<Vec<u8>>, data: Vec<u8>) -> Log {
    Log {
        address: address.to_vec(),
        topics,
        data,
        ..Default::default()
    }
}

/// A successful transaction whose root call (a CALL to `to`) emitted `logs`;
/// the receipt carries the same logs. Logs are numbered in the given order:
/// ordinal `10 + i`, index `i` and block index `100 + i`.
pub fn trx(to: &[u8], logs: Vec<Log>) -> TransactionTrace {
    let logs: Vec<Log> = logs
        .into_iter()
        .enumerate()
        .map(|(i, mut log)| {
            log.ordinal = 10 + i as u64;
            log.index = i as u32;
            log.block_index = 100 + i as u32;
            log
        })
        .collect();
    TransactionTrace {
        status: TransactionTraceStatus::Succeeded.into(),
        from: addr(0xf0),
        to: to.to_vec(),
        hash: vec![0xaa; 32],
        nonce: 7,
        gas_limit: 21000,
        calls: vec![Call {
            index: 1,
            call_type: CallType::Call.into(),
            caller: addr(0xf0),
            address: to.to_vec(),
            begin_ordinal: 1,
            end_ordinal: 99,
            logs: logs.clone(),
            ..Default::default()
        }],
        receipt: TransactionReceipt {
            cumulative_gas_used: 4242,
            logs,
            ..Default::default()
        }
        .into(),
        ..Default::default()
    }
}

/// `trx` without call traces: its logs come from the receipt, with no call.
pub fn without_calls(mut trx: TransactionTrace) -> TransactionTrace {
    trx.calls.clear();
    trx
}

/// `trx` with every call reverted: its logs are not part of the chain state.
pub fn reverted(mut trx: TransactionTrace) -> TransactionTrace {
    for call in &mut trx.calls {
        call.state_reverted = true;
    }
    trx
}

/// `trx` with a failed status.
pub fn failed(mut trx: TransactionTrace) -> TransactionTrace {
    trx.status = TransactionTraceStatus::Failed.into();
    trx
}

/// `trx` without a receipt.
pub fn without_receipt(mut trx: TransactionTrace) -> TransactionTrace {
    trx.receipt = Default::default();
    trx
}

/// A block holding `transaction_traces`.
pub fn block(transaction_traces: Vec<TransactionTrace>) -> Block {
    Block {
        number: 1,
        transaction_traces,
        ..Default::default()
    }
}

/// Tests what every per-protocol package's `project` does alike, given an
/// expression that builds a log the package decodes: the transaction, log and
/// call metadata it copies, the receipt fallback, the transactions it leaves
/// out, and the error on a missing receipt. Expands to two `#[test]`
/// functions in the calling module, which must have the package's `project`
/// and `pb` in scope.
#[macro_export]
macro_rules! transaction_tests {
    ($sample:expr) => {
        #[test]
        fn copies_metadata_and_keeps_successful_transactions_with_events() {
            use $crate::testing::*;
            let sample = || -> eth::Log { $sample };
            let block = block(vec![
                trx(&addr(0x01), vec![sample()]),
                without_calls(trx(&addr(0x02), vec![sample()])),
                reverted(trx(&addr(0x03), vec![sample()])),
                failed(trx(&addr(0x04), vec![sample()])),
                trx(&addr(0x05), vec![log(&addr(0x06), vec![topic0("Unknown()")], vec![])]),
            ]);

            let events = project(&block).expect("project");

            let kept: Vec<_> = events.transactions.iter().map(|transaction| transaction.to.clone()).collect();
            assert_eq!(kept, vec![Some(addr(0x01)), Some(addr(0x02))]);

            let transaction = &events.transactions[0];
            assert_eq!(transaction.hash, vec![0xaa; 32]);
            assert_eq!(transaction.from, addr(0xf0));
            assert_eq!(transaction.nonce, 7);
            assert_eq!(transaction.gas_price, "0");
            assert_eq!(transaction.gas_limit, 21000);
            // v0.8.0 parity: the receipt's cumulative gas.
            assert_eq!(transaction.gas_used, 4242);
            assert_eq!(transaction.value, "0");

            let raw = &block.transaction_traces[0].calls[0].logs[0];
            let entry = &transaction.logs[0];
            assert_eq!(entry.address, raw.address);
            assert_eq!(entry.ordinal, raw.ordinal);
            assert_eq!(entry.topics, raw.topics);
            assert_eq!(entry.data, raw.data);
            assert_eq!(entry.block_index, raw.block_index);
            assert!(entry.log.is_some());
            let call = entry.call.as_option().expect("call metadata");
            assert_eq!(call.index, 1);
            assert_eq!(call.caller, addr(0xf0));
            assert_eq!(call.address, addr(0x01));
            assert_eq!(call.call_type, pb::CallType::Call);
            assert_eq!((call.begin_ordinal, call.end_ordinal), (1, 99));

            // Without call traces, the receipt's logs, with no call.
            let entry = &events.transactions[1].logs[0];
            assert_eq!(entry.ordinal, raw.ordinal);
            assert!(entry.call.is_unset());
        }

        #[test]
        fn fails_on_a_successful_transaction_without_receipt() {
            use $crate::testing::*;
            let sample = || -> eth::Log { $sample };

            let error = project(&block(vec![without_receipt(trx(&addr(0x01), vec![sample()]))])).unwrap_err();
            assert_eq!(error.to_string(), "successful transaction has no receipt");

            // A failed transaction is never read, receipt or not.
            let events = project(&block(vec![failed(without_receipt(trx(&addr(0x01), vec![sample()])))])).expect("project");
            assert!(events.transactions.is_empty());
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic0_is_the_keccak_of_the_signature() {
        // ERC-20 Transfer, a well-known topic 0.
        assert_eq!(
            topic0("Transfer(address,address,uint256)"),
            substreams::hex!("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef").to_vec()
        );
    }

    #[test]
    fn encode_writes_offsets_and_tails() {
        // f(uint256, bytes, uint256[]) with 1, "ab", [7, 8].
        let data = encode(&[
            Abi::Word(uint(1)),
            Abi::Bytes(b"ab".to_vec()),
            Abi::Array(vec![Abi::Word(uint(7)), Abi::Word(uint(8))]),
        ]);
        let mut ab = b"ab".to_vec();
        ab.resize(32, 0);
        let expected = [uint(1), uint(96), uint(160), uint(2), ab, uint(2), uint(7), uint(8)].concat();
        assert_eq!(data, expected);
    }

    #[test]
    fn encode_inlines_static_tuples_and_points_at_dynamic_ones() {
        let data = encode(&[Abi::Tuple(vec![Abi::Word(uint(1)), Abi::Word(uint(2))]), Abi::Tuple(vec![Abi::Bytes(vec![])])]);
        let expected = [uint(1), uint(2), uint(96), uint(32), uint(0)].concat();
        assert_eq!(data, expected);
    }
}
