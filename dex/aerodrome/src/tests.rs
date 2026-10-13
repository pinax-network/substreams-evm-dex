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
            topic0("Swap(address,address,uint256,uint256,uint256,uint256)"),
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
            to: addr(0xa2),
            amount0_in: "1".into(),
            amount1_in: "2".into(),
            amount0_out: "3".into(),
            amount1_out: "4".into(),
        })
    );
}

#[test]
fn sync() {
    let log = log(&addr(0x11), vec![topic0("Sync(uint256,uint256)")], [uint(5), uint(6)].concat());
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Sync {
            reserve0: "5".into(),
            reserve1: "6".into(),
        })
    );
}

#[test]
fn mint() {
    let log = log(
        &addr(0x11),
        vec![topic0("Mint(address,uint256,uint256)"), word(&addr(0xa1))],
        [uint(7), uint(8)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Mint {
            sender: addr(0xa1),
            amount0: "7".into(),
            amount1: "8".into(),
        })
    );
}

#[test]
fn burn() {
    let log = log(
        &addr(0x11),
        vec![topic0("Burn(address,address,uint256,uint256)"), word(&addr(0xa1)), word(&addr(0xa2))],
        [uint(9), uint(10)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Burn {
            sender: addr(0xa1),
            to: addr(0xa2),
            amount0: "9".into(),
            amount1: "10".into(),
        })
    );
}

#[test]
fn fees() {
    let log = log(
        &addr(0x11),
        vec![topic0("Fees(address,uint256,uint256)"), word(&addr(0xa1))],
        [uint(11), uint(12)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Fees {
            sender: addr(0xa1),
            amount0: "11".into(),
            amount1: "12".into(),
        })
    );
}

#[test]
fn claim() {
    let log = log(
        &addr(0x11),
        vec![topic0("Claim(address,address,uint256,uint256)"), word(&addr(0xa1)), word(&addr(0xa2))],
        [uint(13), uint(14)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Claim {
            sender: addr(0xa1),
            recipient: addr(0xa2),
            amount0: "13".into(),
            amount1: "14".into(),
        })
    );
}

#[test]
fn pool_created() {
    let log = log(
        &addr(0x12),
        vec![
            topic0("PoolCreated(address,address,bool,address,uint256)"),
            word(&addr(0xb1)),
            word(&addr(0xb2)),
            boolean(true),
        ],
        [word(&addr(0xb3)), uint(77)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::PoolCreated {
            token0: addr(0xb1),
            token1: addr(0xb2),
            stable: true,
            pool: addr(0xb3),
            extra_data: "77".into(),
        })
    );
}
