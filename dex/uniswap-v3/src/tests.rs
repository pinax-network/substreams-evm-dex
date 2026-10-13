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

fn swap_log(tick: i128) -> Log {
    log(
        &addr(0x11),
        vec![
            topic0("Swap(address,address,int256,int256,uint160,uint128,int24)"),
            word(&addr(0xa1)),
            word(&addr(0xa2)),
        ],
        [int(-1_000), int(2_000), uint(1 << 96), uint(5_000), int(tick)].concat(),
    )
}

common::transaction_tests!(swap_log(60));

#[test]
fn swap() {
    assert_eq!(
        decode(swap_log(887_272)),
        pb::log::Log::from(pb::Swap {
            sender: addr(0xa1),
            recipient: addr(0xa2),
            amount0: "-1000".into(),
            amount1: "2000".into(),
            sqrt_price_x96: (1u128 << 96).to_string(),
            liquidity: "5000".into(),
            tick: 887_272,
        })
    );
}

#[test]
fn negative_ticks_are_emitted_as_zero_as_in_v0_8_0() {
    // v0.8.0's bigint_to_i32 rejected negative values; see the README.
    let pb::log::Log::Swap(swap) = decode(swap_log(-200)) else { panic!("Swap") };
    assert_eq!(swap.tick, 0);
}

#[test]
fn initialize() {
    let log = log(
        &addr(0x11),
        vec![topic0("Initialize(uint160,int24)")],
        [uint(79_228_162_514_264_337_593_543_950_336), int(0)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Initialize {
            sqrt_price_x96: "79228162514264337593543950336".into(),
            tick: 0,
        })
    );
}

#[test]
fn mint() {
    let log = log(
        &addr(0x11),
        vec![
            topic0("Mint(address,address,int24,int24,uint128,uint256,uint256)"),
            word(&addr(0xa2)),
            int(-60),
            int(60),
        ],
        [word(&addr(0xa1)), uint(10), uint(11), uint(12)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Mint {
            sender: addr(0xa1),
            owner: addr(0xa2),
            tick_lower: 0, // -60 under v0.8.0's bigint_to_i32
            tick_upper: 60,
            amount: "10".into(),
            amount0: "11".into(),
            amount1: "12".into(),
        })
    );
}

#[test]
fn collect() {
    let log = log(
        &addr(0x11),
        vec![
            topic0("Collect(address,address,int24,int24,uint128,uint128)"),
            word(&addr(0xa1)),
            int(120),
            int(240),
        ],
        [word(&addr(0xa2)), uint(13), uint(14)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Collect {
            owner: addr(0xa1),
            recipient: addr(0xa2),
            tick_lower: 120,
            tick_upper: 240,
            amount0: "13".into(),
            amount1: "14".into(),
        })
    );
}

#[test]
fn burn() {
    let log = log(
        &addr(0x11),
        vec![topic0("Burn(address,int24,int24,uint128,uint256,uint256)"), word(&addr(0xa1)), int(10), int(20)],
        [uint(15), uint(16), uint(17)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Burn {
            owner: addr(0xa1),
            tick_lower: 10,
            tick_upper: 20,
            amount: "15".into(),
            amount0: "16".into(),
            amount1: "17".into(),
        })
    );
}

#[test]
fn flash() {
    let log = log(
        &addr(0x11),
        vec![
            topic0("Flash(address,address,uint256,uint256,uint256,uint256)"),
            word(&addr(0xa1)),
            word(&addr(0xa2)),
        ],
        [uint(18), uint(19), uint(20), uint(21)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Flash {
            sender: addr(0xa1),
            recipient: addr(0xa2),
            amount0: "18".into(),
            amount1: "19".into(),
            paid0: "20".into(),
            paid1: "21".into(),
        })
    );
}

#[test]
fn increase_observation_cardinality_next() {
    let log = log(
        &addr(0x11),
        vec![topic0("IncreaseObservationCardinalityNext(uint16,uint16)")],
        [uint(1), uint(65_535)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::IncreaseObservationCardinalityNext {
            observation_cardinality_next_old: 1,
            observation_cardinality_next_new: 65_535,
        })
    );
}

#[test]
fn set_fee_protocol() {
    let log = log(
        &addr(0x11),
        vec![topic0("SetFeeProtocol(uint8,uint8,uint8,uint8)")],
        [uint(0), uint(4), uint(5), uint(10)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::SetFeeProtocol {
            fee_protocol0_old: 0,
            fee_protocol1_old: 4,
            fee_protocol0_new: 5,
            fee_protocol1_new: 10,
        })
    );
}

#[test]
fn collect_protocol() {
    let log = log(
        &addr(0x11),
        vec![topic0("CollectProtocol(address,address,uint128,uint128)"), word(&addr(0xa1)), word(&addr(0xa2))],
        [uint(22), uint(23)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::CollectProtocol {
            sender: addr(0xa1),
            recipient: addr(0xa2),
            amount0: "22".into(),
            amount1: "23".into(),
        })
    );
}

#[test]
fn pool_created() {
    let log = log(
        &addr(0x12),
        vec![
            topic0("PoolCreated(address,address,uint24,int24,address)"),
            word(&addr(0xb1)),
            word(&addr(0xb2)),
            uint(3_000),
        ],
        [int(60), word(&addr(0xb3))].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::PoolCreated {
            pool: addr(0xb3),
            token0: addr(0xb1),
            token1: addr(0xb2),
            fee: 3_000,
            tick_spacing: 60,
        })
    );
}

#[test]
fn owner_changed() {
    let log = log(
        &addr(0x12),
        vec![topic0("OwnerChanged(address,address)"), word(&addr(0xc1)), word(&addr(0xc2))],
        vec![],
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::OwnerChanged {
            old_owner: addr(0xc1),
            new_owner: addr(0xc2),
        })
    );
}

#[test]
fn fee_amount_enabled() {
    let log = log(&addr(0x12), vec![topic0("FeeAmountEnabled(uint24,int24)"), uint(100), int(1)], vec![]);
    assert_eq!(decode(log), pb::log::Log::from(pb::FeeAmountEnabled { fee: 100, tick_spacing: 1 }));
}
