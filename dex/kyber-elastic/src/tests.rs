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
        [int(500), int(-498), uint(1 << 96), uint(10_000), int(tick)].concat(),
    )
}

fn pool_created_log(swap_fee_units: Vec<u8>) -> Log {
    log(
        &addr(0x12),
        vec![
            topic0("PoolCreated(address,address,uint24,int24,address)"),
            word(&addr(0xb1)),
            word(&addr(0xb2)),
            swap_fee_units,
        ],
        [int(8), word(&addr(0xb3))].concat(),
    )
}

common::transaction_tests!(swap_log(1));

#[test]
fn swap() {
    assert_eq!(
        decode(swap_log(100)),
        pb::log::Log::from(pb::Swap {
            sender: addr(0xa1),
            recipient: addr(0xa2),
            delta_qty0: "500".into(),
            delta_qty1: "-498".into(),
            sqrt_p: (1u128 << 96).to_string(),
            liquidity: "10000".into(),
            current_tick: 100,
        })
    );
}

#[test]
fn negative_ticks_are_emitted_as_zero_as_in_v0_8_0() {
    let pb::log::Log::Swap(swap) = decode(swap_log(-100)) else { panic!("Swap") };
    assert_eq!(swap.current_tick, 0);
}

#[test]
fn mint() {
    let log = log(
        &addr(0x11),
        vec![
            topic0("Mint(address,address,int24,int24,uint128,uint256,uint256)"),
            word(&addr(0xa2)),
            int(-8),
            int(8),
        ],
        [word(&addr(0xa1)), uint(1), uint(2), uint(3)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Mint {
            sender: addr(0xa1),
            owner: addr(0xa2),
            tick_lower: 0, // -8 under v0.8.0's bigint_to_i32
            tick_upper: 8,
            qty: "1".into(),
            qty0: "2".into(),
            qty1: "3".into(),
        })
    );
}

#[test]
fn burn() {
    let log = log(
        &addr(0x11),
        vec![topic0("Burn(address,int24,int24,uint128,uint256,uint256)"), word(&addr(0xa1)), int(16), int(24)],
        [uint(4), uint(5), uint(6)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Burn {
            owner: addr(0xa1),
            tick_lower: 16,
            tick_upper: 24,
            qty: "4".into(),
            qty0: "5".into(),
            qty1: "6".into(),
        })
    );
}

#[test]
fn burn_r_tokens() {
    let log = log(
        &addr(0x11),
        vec![topic0("BurnRTokens(address,uint256,uint256,uint256)"), word(&addr(0xa1))],
        [uint(7), uint(8), uint(9)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::BurnRTokens {
            owner: addr(0xa1),
            qty: "7".into(),
            qty0: "8".into(),
            qty1: "9".into(),
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
        [uint(10), uint(11), uint(12), uint(13)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Flash {
            sender: addr(0xa1),
            recipient: addr(0xa2),
            qty0: "10".into(),
            qty1: "11".into(),
            paid0: "12".into(),
            paid1: "13".into(),
        })
    );
}

#[test]
fn initialize() {
    let log = log(&addr(0x11), vec![topic0("Initialize(uint160,int24)")], [uint(1 << 96), int(42)].concat());
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Initialize {
            sqrt_p: (1u128 << 96).to_string(),
            tick: 42,
        })
    );
}

#[test]
fn pool_created() {
    assert_eq!(
        decode(pool_created_log(uint(40))),
        pb::log::Log::from(pb::PoolCreated {
            token0: addr(0xb1),
            token1: addr(0xb2),
            swap_fee_units: 40,
            tick_distance: 8,
            pool: addr(0xb3),
        })
    );
}

#[test]
fn swap_fee_units_wrap_into_u32_and_fail_above_u64_max() {
    // v0.8.0: `swap_fee_units.to_u64() as u32` keeps the low 32 bits...
    let pb::log::Log::PoolCreated(event) = decode(pool_created_log(uint((1 << 32) + 40))) else {
        panic!("PoolCreated")
    };
    assert_eq!(event.swap_fee_units, 40);

    // ...and `to_u64()` panicked above u64::MAX; the port fails the block.
    let error = project(&block(vec![trx(&addr(0x01), vec![pool_created_log(uint(1 << 64))])])).unwrap_err();
    assert_eq!(
        error.to_string(),
        "PoolCreated.swapFeeUnits: BigInt '18446744073709551616' is too large to fit into u64"
    );
}
