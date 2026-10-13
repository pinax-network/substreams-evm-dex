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

fn swap_log() -> Log {
    log(
        &addr(0x11),
        vec![
            topic0("Swap(address,uint256,uint256,uint256,uint256,address)"),
            word(&addr(0xa1)),
            word(&addr(0xa2)),
        ],
        [uint(1), uint(2), uint(3), uint(4)].concat(),
    )
}

common::transaction_tests!(swap_log());

#[test]
fn swap() {
    assert_eq!(
        decode(swap_log()),
        pb::log::Log::from(pb::Swap {
            sender: addr(0xa1),
            amount0_in: "1".into(),
            amount1_in: "2".into(),
            amount0_out: "3".into(),
            amount1_out: "4".into(),
            to: addr(0xa2),
        })
    );
}

#[test]
fn mint() {
    let log = log(
        &addr(0x11),
        vec![topic0("Mint(address,uint256,uint256)"), word(&addr(0xa1))],
        [uint(5), uint(6)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Mint {
            sender: addr(0xa1),
            amount0: "5".into(),
            amount1: "6".into(),
        })
    );
}

#[test]
fn burn() {
    let log = log(
        &addr(0x11),
        vec![topic0("Burn(address,uint256,uint256,address)"), word(&addr(0xa1)), word(&addr(0xa2))],
        [uint(7), uint(8)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Burn {
            sender: addr(0xa1),
            amount0: "7".into(),
            amount1: "8".into(),
            to: addr(0xa2),
        })
    );
}

#[test]
fn sync() {
    let log = log(&addr(0x11), vec![topic0("Sync(uint112,uint112)")], [uint(9), uint(1 << 111)].concat());
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Sync {
            reserve0: "9".into(),
            reserve1: (1u128 << 111).to_string(),
        })
    );
}

#[test]
fn pair_created() {
    let log = log(
        &addr(0x12),
        vec![topic0("PairCreated(address,address,address,uint256)"), word(&addr(0xb1)), word(&addr(0xb2))],
        [word(&addr(0xb3)), uint(42)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::PairCreated {
            pair: addr(0xb3),
            token0: addr(0xb1),
            token1: addr(0xb2),
            extra_data: "42".into(),
        })
    );
}

#[test]
fn a_log_with_extra_data_does_not_match() {
    // Static events must carry exactly their data, as in v0.8.0's decoders.
    let mut log = swap_log();
    log.data.extend(uint(0));
    let events = project(&block(vec![trx(&addr(0x01), vec![log])])).expect("project");
    assert!(events.transactions.is_empty());
}
