//! One synthetic log per decoded event; topic 0 is the Keccak-256 of the
//! canonical signature from the substreams-abis ABI JSON.

use super::*;
use common::testing::*;

/// The event `project` decodes from a block holding only `log`.
fn decode(log: Log) -> pb::log::Log {
    let events = project(&block(vec![trx(&addr(0x01), vec![log])])).expect("project");
    let [transaction] = events.transactions.as_slice() else {
        panic!("one transaction")
    };
    let [entry] = transaction.logs.as_slice() else { panic!("one decoded log") };
    entry.log.clone().expect("event")
}

const SETTLEMENT: u8 = 0x9e;

/// An order UID: a 32-byte digest, the owner and a 4-byte validity.
fn order_uid() -> Vec<u8> {
    [vec![0x0d; 32], addr(0xa1), vec![0x65, 0x00, 0x00, 0x00]].concat()
}

fn trade_log() -> Log {
    log(
        &addr(SETTLEMENT),
        vec![topic0("Trade(address,address,address,uint256,uint256,uint256,bytes)"), word(&addr(0xa1))],
        encode(&[
            Abi::Word(word(&addr(0xb1))),
            Abi::Word(word(&addr(0xb2))),
            Abi::Word(uint(1_000)),
            Abi::Word(uint(2_000)),
            Abi::Word(uint(3)),
            Abi::Bytes(order_uid()),
        ]),
    )
}

common::transaction_tests!(trade_log());

#[test]
fn interaction() {
    let log = log(
        &addr(SETTLEMENT),
        vec![topic0("Interaction(address,uint256,bytes4)"), word(&addr(0xc1))],
        [uint(5), fixed(&[0xa9, 0x05, 0x9c, 0xbb])].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Interaction {
            target: addr(0xc1),
            value: "5".into(),
            selector: vec![0xa9, 0x05, 0x9c, 0xbb],
        })
    );
}

#[test]
fn order_invalidated() {
    let log = log(
        &addr(SETTLEMENT),
        vec![topic0("OrderInvalidated(address,bytes)"), word(&addr(0xa1))],
        encode(&[Abi::Bytes(order_uid())]),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::OrderInvalidated {
            owner: addr(0xa1),
            order_uid: order_uid(),
        })
    );
}

#[test]
fn pre_signature() {
    let log = log(
        &addr(SETTLEMENT),
        vec![topic0("PreSignature(address,bytes,bool)"), word(&addr(0xa1))],
        encode(&[Abi::Bytes(order_uid()), Abi::Word(boolean(true))]),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::PreSignature {
            owner: addr(0xa1),
            order_uid: order_uid(),
            signed: true,
        })
    );
}

#[test]
fn settlement() {
    let log = log(&addr(SETTLEMENT), vec![topic0("Settlement(address)"), word(&addr(0xd1))], vec![]);
    assert_eq!(decode(log), pb::log::Log::from(pb::Settlement { solver: addr(0xd1) }));
}

#[test]
fn trade() {
    assert_eq!(
        decode(trade_log()),
        pb::log::Log::from(pb::Trade {
            owner: addr(0xa1),
            sell_token: addr(0xb1),
            buy_token: addr(0xb2),
            sell_amount: "1000".into(),
            buy_amount: "2000".into(),
            fee_amount: "3".into(),
            order_uid: order_uid(),
        })
    );
}
