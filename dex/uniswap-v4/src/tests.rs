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

const POOL_ID: [u8; 32] = [0x5a; 32];

fn swap_log() -> Log {
    log(
        &addr(0x11),
        vec![
            topic0("Swap(bytes32,address,int128,int128,uint160,uint128,int24,uint24)"),
            POOL_ID.to_vec(),
            word(&addr(0xa1)),
        ],
        [int(-100), int(99), uint(1 << 96), uint(1_000), int(10), uint(500)].concat(),
    )
}

common::transaction_tests!(swap_log());

#[test]
fn swap() {
    assert_eq!(
        decode(swap_log()),
        pb::log::Log::from(pb::Swap {
            id: POOL_ID.to_vec(),
            sender: addr(0xa1),
            amount0: "-100".into(),
            amount1: "99".into(),
            sqrt_price_x96: (1u128 << 96).to_string(),
            liquidity: "1000".into(),
            tick: 10,
            fee: "500".into(),
        })
    );
}

#[test]
fn initialize() {
    let log = log(
        &addr(0x11),
        vec![
            topic0("Initialize(bytes32,address,address,uint24,int24,address,uint160,int24)"),
            POOL_ID.to_vec(),
            word(&addr(0xb1)),
            word(&addr(0xb2)),
        ],
        [uint(3_000), int(60), word(&addr(0xb3)), uint(1 << 96), int(-5)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Initialize {
            id: POOL_ID.to_vec(),
            currency0: addr(0xb1),
            currency1: addr(0xb2),
            fee: 3_000,
            tick_spacing: 60,
            hooks: addr(0xb3),
            sqrt_price_x96: (1u128 << 96).to_string(),
            tick: 0, // -5 under v0.8.0's bigint_to_i32
        })
    );
}

#[test]
fn modify_liquidity() {
    let log = log(
        &addr(0x11),
        vec![
            topic0("ModifyLiquidity(bytes32,address,int24,int24,int256,bytes32)"),
            POOL_ID.to_vec(),
            word(&addr(0xa1)),
        ],
        [int(100), int(200), int(-7), vec![0x77; 32]].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::ModifyLiquidity {
            id: POOL_ID.to_vec(),
            sender: addr(0xa1),
            tick_lower: 100,
            tick_upper: 200,
            liquidity_delta: "-7".into(),
            salt: vec![0x77; 32],
        })
    );
}

#[test]
fn donate() {
    let log = log(
        &addr(0x11),
        vec![topic0("Donate(bytes32,address,uint256,uint256)"), POOL_ID.to_vec(), word(&addr(0xa1))],
        [uint(8), uint(9)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Donate {
            id: POOL_ID.to_vec(),
            sender: addr(0xa1),
            amount0: "8".into(),
            amount1: "9".into(),
        })
    );
}

#[test]
fn protocol_fee_controller_updated() {
    let log = log(&addr(0x11), vec![topic0("ProtocolFeeControllerUpdated(address)"), word(&addr(0xc1))], vec![]);
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::ProtocolFeeControllerUpdated {
            protocol_fee_controller: addr(0xc1),
        })
    );
}

#[test]
fn protocol_fee_updated() {
    let log = log(&addr(0x11), vec![topic0("ProtocolFeeUpdated(bytes32,uint24)"), POOL_ID.to_vec()], uint(1_000));
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::ProtocolFeeUpdated {
            id: POOL_ID.to_vec(),
            protocol_fee: 1_000,
        })
    );
}
