//! Synthetic-log tests of every protocol decoder and of `project`. Expected
//! values follow the v0.8.0 (`9663b25`) decoders line by line, including the
//! behaviour the README lists as kept on purpose; those tests say so.

use std::cell::RefCell;

use super::*;
use crate::test_support::*;
use buffa::Message;
use substreams_ethereum::pb::eth::v2::{Call, CallType, TransactionTraceStatus};
use substreams_ethereum::NULL_ADDRESS;

const TX_FROM: u8 = 0xf0;
const SENDER: u8 = 0xcc;
const FACTORY: u8 = 0xfa;
const T0: u8 = 0x10;
const T1: u8 = 0x11;
const T2: u8 = 0x12;
const V1: u8 = 0x51;
const V2: u8 = 0x52;
const V3: u8 = 0x53;
const AERO: u8 = 0x54;
const KYBER: u8 = 0x55;
const TJ: u8 = 0x56;
const CURVE: u8 = 0x57;
const BAL: u8 = 0x58;
const BANCOR: u8 = 0x59;
const SUN_TOKEN: u8 = 0x5a;
const UNKNOWN: u8 = 0x99;
const VAULT: u8 = 0xba;
const BAL_FACTORY: u8 = 0xbf;
const POOL_MANAGER: u8 = 0x4d;
const ROOT: u8 = 0xee;

fn v4_id() -> Vec<u8> {
    vec![0x44; 32]
}

fn pools() -> PoolMetadataMap {
    let pair = || vec![addr(T0), addr(T1)];
    let mut pools = HashMap::new();
    for (address, tokens, factory) in [
        (addr(V1), vec![addr(T0)], addr(FACTORY)),
        (addr(V2), pair(), addr(FACTORY)),
        (addr(V3), pair(), addr(FACTORY)),
        (addr(AERO), pair(), addr(FACTORY)),
        (addr(KYBER), pair(), addr(FACTORY)),
        (addr(TJ), pair(), addr(FACTORY)),
        (addr(CURVE), vec![addr(T0), addr(T1), addr(T2)], addr(FACTORY)),
        (addr(BAL), pair(), addr(BAL_FACTORY)),
        (addr(BANCOR), vec![], addr(FACTORY)),
        (addr(SUN_TOKEN), vec![], addr(FACTORY)),
        (v4_id(), pair(), addr(POOL_MANAGER)),
    ] {
        pools.insert(address, Pool { tokens, factory });
    }
    pools
}

fn tx() -> TransactionTrace {
    trx(&addr(ROOT), vec![])
}

fn decode(log: &Log) -> Vec<pb::log::Log> {
    decode_log(&tx(), log, &pools()).unwrap()
}

fn swap(protocol: pb::Protocol, factory: &[u8], pool: &[u8], user: &[u8], input: (&[u8], &str), output: (&[u8], &str)) -> pb::log::Log {
    pb::log::Log::from(pb::Swap {
        protocol: protocol.into(),
        factory: factory.to_vec(),
        pool: pool.to_vec(),
        user: user.to_vec(),
        input_token: input.0.to_vec(),
        input_amount: input.1.to_string(),
        output_token: output.0.to_vec(),
        output_amount: output.1.to_string(),
    })
}

fn fee(protocol: pb::Protocol, factory: &[u8], pool: &[u8], fee: u32) -> pb::log::Log {
    pb::log::Log::from(pb::SwapFee {
        protocol: protocol.into(),
        factory: factory.to_vec(),
        pool: pool.to_vec(),
        fee,
    })
}

fn init(protocol: pb::Protocol, factory: &[u8], pool: &[u8]) -> pb::log::Log {
    pb::log::Log::from(pb::Initialize {
        protocol: protocol.into(),
        factory: factory.to_vec(),
        pool: pool.to_vec(),
    })
}

// ── Uniswap V1 ───────────────────────────────────────────────────────────────────────────

fn v1_trade(signature: &str, emitter: u8) -> Log {
    log(&addr(emitter), vec![topic0(signature), word(&addr(SENDER)), uint(100), uint(200)], vec![])
}

#[test]
fn uniswap_v1_trades_price_against_native_eth() {
    assert_eq!(
        decode(&v1_trade("TokenPurchase(address,uint256,uint256)", V1)),
        vec![swap(
            pb::Protocol::UniswapV1,
            &addr(FACTORY),
            &addr(V1),
            &addr(TX_FROM),
            (&NULL_ADDRESS, "100"),
            (&addr(T0), "200")
        )]
    );
    assert_eq!(
        decode(&v1_trade("EthPurchase(address,uint256,uint256)", V1)),
        vec![swap(
            pb::Protocol::UniswapV1,
            &addr(FACTORY),
            &addr(V1),
            &addr(TX_FROM),
            (&addr(T0), "100"),
            (&NULL_ADDRESS, "200")
        )]
    );
    assert!(decode(&v1_trade("TokenPurchase(address,uint256,uint256)", UNKNOWN)).is_empty());
}

#[test]
fn uniswap_v1_new_exchange_initializes_with_a_3000_fee() {
    let log = log(
        &addr(FACTORY),
        vec![topic0("NewExchange(address,address)"), word(&addr(T0)), word(&addr(V1))],
        vec![],
    );
    assert_eq!(
        decode(&log),
        vec![
            init(pb::Protocol::UniswapV1, &addr(FACTORY), &addr(V1)),
            fee(pb::Protocol::UniswapV1, &addr(FACTORY), &addr(V1), 3000)
        ]
    );
}

// ── Uniswap V2 and Aerodrome ─────────────────────────────────────────────────────────────

fn v2_swap(emitter: u8, amounts: [u128; 4]) -> Log {
    log(
        &addr(emitter),
        vec![
            topic0("Swap(address,uint256,uint256,uint256,uint256,address)"),
            word(&addr(SENDER)),
            word(&addr(0x70)),
        ],
        concat(&amounts.map(uint)),
    )
}

#[test]
fn uniswap_v2_swap_direction_follows_the_non_zero_amounts() {
    let user = addr(TX_FROM);
    assert_eq!(
        decode(&v2_swap(V2, [100, 0, 0, 90])),
        vec![swap(
            pb::Protocol::UniswapV2,
            &addr(FACTORY),
            &addr(V2),
            &user,
            (&addr(T0), "100"),
            (&addr(T1), "90")
        )]
    );
    assert_eq!(
        decode(&v2_swap(V2, [0, 100, 90, 0])),
        vec![swap(
            pb::Protocol::UniswapV2,
            &addr(FACTORY),
            &addr(V2),
            &user,
            (&addr(T1), "100"),
            (&addr(T0), "90")
        )]
    );
    // Amounts beyond u128 stay exact decimal strings.
    let mut large = v2_swap(V2, [0, 0, 0, 90]);
    large.data[..32].copy_from_slice(&[0xff; 32]);
    let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    assert_eq!(
        decode(&large),
        vec![swap(
            pb::Protocol::UniswapV2,
            &addr(FACTORY),
            &addr(V2),
            &user,
            (&addr(T0), max),
            (&addr(T1), "90")
        )]
    );
}

#[test]
fn uniswap_v2_ambiguous_swaps_and_unknown_pairs_are_dropped() {
    for amounts in [[100, 100, 0, 90], [100, 0, 90, 0], [0, 0, 0, 0], [100, 0, 1, 90]] {
        assert!(decode(&v2_swap(V2, amounts)).is_empty(), "{amounts:?}");
    }
    assert!(decode(&v2_swap(UNKNOWN, [100, 0, 0, 90])).is_empty());
}

#[test]
fn uniswap_v2_pair_created_initializes_with_a_3000_fee() {
    let log = log(
        &addr(FACTORY),
        vec![topic0("PairCreated(address,address,address,uint256)"), word(&addr(T0)), word(&addr(T1))],
        concat(&[word(&addr(V2)), uint(1)]),
    );
    assert_eq!(
        decode(&log),
        vec![
            init(pb::Protocol::UniswapV2, &addr(FACTORY), &addr(V2)),
            fee(pb::Protocol::UniswapV2, &addr(FACTORY), &addr(V2), 3000)
        ]
    );
}

#[test]
fn aerodrome_swap_reports_the_event_sender() {
    let log = log(
        &addr(AERO),
        vec![
            topic0("Swap(address,address,uint256,uint256,uint256,uint256)"),
            word(&addr(SENDER)),
            word(&addr(0x70)),
        ],
        concat(&[uint(0), uint(100), uint(90), uint(0)]),
    );
    assert_eq!(
        decode(&log),
        vec![swap(
            pb::Protocol::Aerodrome,
            &addr(FACTORY),
            &addr(AERO),
            &addr(SENDER),
            (&addr(T1), "100"),
            (&addr(T0), "90")
        )]
    );
}

#[test]
fn aerodrome_pool_created_initializes_without_a_fee() {
    let log = log(
        &addr(FACTORY),
        vec![
            topic0("PoolCreated(address,address,bool,address,uint256)"),
            word(&addr(T0)),
            word(&addr(T1)),
            uint(0),
        ],
        concat(&[word(&addr(AERO)), uint(1)]),
    );
    assert_eq!(decode(&log), vec![init(pb::Protocol::Aerodrome, &addr(FACTORY), &addr(AERO))]);
}

// ── Uniswap V3 and Kyber Elastic (one event signature) ───────────────────────────────────

fn v3_swap(emitter: u8, amount0: i128, amount1: i128) -> Log {
    log(
        &addr(emitter),
        vec![
            topic0("Swap(address,address,int256,int256,uint160,uint128,int24)"),
            word(&addr(SENDER)),
            word(&addr(0x70)),
        ],
        concat(&[int(amount0), int(amount1), uint(1 << 96), uint(1000), int(-120)]),
    )
}

/// Uniswap V3's and Kyber Elastic's `Swap` share topic 0 and layout, and
/// v0.8.0 runs every decoder on every log: a swap in any stored pool yields a
/// V3 row (positive amount = input, `user` = transaction sender) and a Kyber
/// row (negative amount = input, `user` = event sender). Kept for parity.
#[test]
fn v3_shaped_swaps_yield_a_uniswap_v3_and_a_kyber_elastic_swap() {
    for pool in [V3, KYBER] {
        let f = addr(FACTORY);
        let p = addr(pool);
        assert_eq!(
            decode(&v3_swap(pool, 100, -90)),
            vec![
                swap(pb::Protocol::UniswapV3, &f, &p, &addr(TX_FROM), (&addr(T0), "100"), (&addr(T1), "90")),
                swap(pb::Protocol::KyberElastic, &f, &p, &addr(SENDER), (&addr(T1), "90"), (&addr(T0), "100")),
            ]
        );
        assert_eq!(
            decode(&v3_swap(pool, -100, 90)),
            vec![
                swap(pb::Protocol::UniswapV3, &f, &p, &addr(TX_FROM), (&addr(T1), "90"), (&addr(T0), "100")),
                swap(pb::Protocol::KyberElastic, &f, &p, &addr(SENDER), (&addr(T0), "100"), (&addr(T1), "90")),
            ]
        );
    }
}

#[test]
fn v3_shaped_swaps_without_opposite_signs_or_pool_are_dropped() {
    for (amount0, amount1) in [(100, 90), (-100, -90), (0, -90), (100, 0), (0, 0)] {
        assert!(decode(&v3_swap(V3, amount0, amount1)).is_empty(), "{amount0} {amount1}");
    }
    assert!(decode(&v3_swap(UNKNOWN, 100, -90)).is_empty());
}

#[test]
fn v3_shaped_pool_created_initializes_both_protocols() {
    let log = log(
        &addr(FACTORY),
        vec![
            topic0("PoolCreated(address,address,uint24,int24,address)"),
            word(&addr(T0)),
            word(&addr(T1)),
            uint(500),
        ],
        concat(&[int(10), word(&addr(V3))]),
    );
    assert_eq!(
        decode(&log),
        vec![
            init(pb::Protocol::UniswapV3, &addr(FACTORY), &addr(V3)),
            fee(pb::Protocol::UniswapV3, &addr(FACTORY), &addr(V3), 500),
            init(pb::Protocol::KyberElastic, &addr(FACTORY), &addr(V3)),
        ]
    );
}

// ── Uniswap V4 ───────────────────────────────────────────────────────────────────────────

fn v4_swap(id: &[u8], amount0: i128, amount1: i128) -> Log {
    log(
        &addr(POOL_MANAGER),
        vec![
            topic0("Swap(bytes32,address,int128,int128,uint160,uint128,int24,uint24)"),
            id.to_vec(),
            word(&addr(SENDER)),
        ],
        concat(&[int(amount0), int(amount1), uint(1 << 96), uint(1000), int(-5), uint(3000)]),
    )
}

/// v0.8.0 reads V4 amounts like V3's (positive = input). V4's `Swap` reports
/// the swapper's balance delta, where the input is negative (substreams-evm
/// 90dff5e says so for its ClickHouse view), so this direction is suspect;
/// kept for parity and listed in the README.
#[test]
fn uniswap_v4_swap_keeps_v0_8_0_direction_positive_amount_is_input() {
    let pm = addr(POOL_MANAGER);
    // A swapper paying 100 token0 for 90 token1.
    assert_eq!(
        decode(&v4_swap(&v4_id(), -100, 90)),
        vec![swap(
            pb::Protocol::UniswapV4,
            &pm,
            &v4_id(),
            &addr(TX_FROM),
            (&addr(T1), "90"),
            (&addr(T0), "100")
        )]
    );
    assert_eq!(
        decode(&v4_swap(&v4_id(), 100, -90)),
        vec![swap(
            pb::Protocol::UniswapV4,
            &pm,
            &v4_id(),
            &addr(TX_FROM),
            (&addr(T0), "100"),
            (&addr(T1), "90")
        )]
    );
    assert!(decode(&v4_swap(&v4_id(), 100, 90)).is_empty());
    assert!(decode(&v4_swap(&[0x45; 32], 100, -90)).is_empty());
}

fn v4_initialize(id: &[u8]) -> Log {
    log(
        &addr(POOL_MANAGER),
        vec![
            topic0("Initialize(bytes32,address,address,uint24,int24,address,uint160,int24)"),
            id.to_vec(),
            word(&addr(T0)),
            word(&addr(T1)),
        ],
        concat(&[uint(500), int(10), word(&addr(0)), uint(1 << 96), int(-7)]),
    )
}

#[test]
fn uniswap_v4_initialize_takes_the_factory_from_the_pool_lookup() {
    let pm = addr(POOL_MANAGER);
    assert_eq!(
        decode(&v4_initialize(&v4_id())),
        vec![init(pb::Protocol::UniswapV4, &pm, &v4_id()), fee(pb::Protocol::UniswapV4, &pm, &v4_id(), 500)]
    );
    let other = vec![0x45; 32];
    assert_eq!(
        decode(&v4_initialize(&other)),
        vec![init(pb::Protocol::UniswapV4, &[], &other), fee(pb::Protocol::UniswapV4, &[], &other, 500)]
    );
}

// ── TraderJoe ────────────────────────────────────────────────────────────────────────────

/// A Liquidity Book `bytes32` packing `x` in the low 128 bits and `y` in the
/// high 128 bits (`PackedUint128Math.encode(x, y)`).
fn packed(x: u128, y: u128) -> Vec<u8> {
    [y.to_be_bytes(), x.to_be_bytes()].concat()
}

fn traderjoe_swap(amounts_in: Vec<u8>, amounts_out: Vec<u8>) -> Log {
    log(
        &addr(TJ),
        vec![
            topic0("Swap(address,address,uint24,bytes32,bytes32,uint24,bytes32,bytes32)"),
            word(&addr(SENDER)),
            word(&addr(0x70)),
        ],
        concat(&[uint(8_388_608), amounts_in, amounts_out, uint(0), vec![0; 32], vec![0; 32]]),
    )
}

/// v0.8.0 reads the high 128 bits of `amountsIn`/`amountsOut` as X and the
/// low 128 bits as Y, the reverse of Liquidity Book's packing, so a swap of X
/// for Y is reported as Y for X with the amounts kept. Kept for parity and
/// listed in the README.
#[test]
fn traderjoe_reads_the_high_half_as_x_as_v0_8_0_did() {
    let (f, p, u) = (addr(FACTORY), addr(TJ), addr(SENDER));
    // Liquidity Book: 100 X in, 90 Y out.
    assert_eq!(
        decode(&traderjoe_swap(packed(100, 0), packed(0, 90))),
        vec![swap(pb::Protocol::Traderjoe, &f, &p, &u, (&addr(T1), "100"), (&addr(T0), "90"))]
    );
    // Liquidity Book: 100 Y in, 90 X out.
    assert_eq!(
        decode(&traderjoe_swap(packed(0, 100), packed(90, 0))),
        vec![swap(pb::Protocol::Traderjoe, &f, &p, &u, (&addr(T0), "100"), (&addr(T1), "90"))]
    );
    assert!(decode(&traderjoe_swap(packed(100, 100), packed(0, 90))).is_empty());
    assert!(decode(&traderjoe_swap(packed(0, 0), packed(0, 90))).is_empty());
}

#[test]
fn traderjoe_lb_pair_created_initializes_without_a_fee() {
    let log = log(
        &addr(FACTORY),
        vec![
            topic0("LBPairCreated(address,address,uint256,address,uint256)"),
            word(&addr(T0)),
            word(&addr(T1)),
            uint(25),
        ],
        concat(&[word(&addr(TJ)), uint(3)]),
    );
    assert_eq!(decode(&log), vec![init(pb::Protocol::Traderjoe, &addr(FACTORY), &addr(TJ))]);
}

// ── CurveFi ──────────────────────────────────────────────────────────────────────────────

fn curve_exchange(signature: &str, sold_id: Vec<u8>, bought_id: Vec<u8>) -> Log {
    log(
        &addr(CURVE),
        vec![topic0(signature), word(&addr(SENDER))],
        concat(&[sold_id, uint(1000), bought_id, uint(990)]),
    )
}

#[test]
fn curvefi_token_exchanges_index_the_pool_tokens() {
    let expected = vec![swap(
        pb::Protocol::Curvefi,
        &addr(FACTORY),
        &addr(CURVE),
        &addr(SENDER),
        (&addr(T0), "1000"),
        (&addr(T2), "990"),
    )];
    assert_eq!(
        decode(&curve_exchange("TokenExchange(address,int128,uint256,int128,uint256)", int(0), int(2))),
        expected
    );
    assert_eq!(
        decode(&curve_exchange("TokenExchange(address,uint256,uint256,uint256,uint256)", uint(0), uint(2))),
        expected
    );

    // An index past the pool's tokens, or a negative int128 index, is dropped.
    assert!(decode(&curve_exchange("TokenExchange(address,int128,uint256,int128,uint256)", int(0), int(3))).is_empty());
    assert!(decode(&curve_exchange("TokenExchange(address,int128,uint256,int128,uint256)", int(-1), int(2))).is_empty());
}

#[test]
fn curvefi_fee_changes_of_stored_pools() {
    let commit = log(
        &addr(CURVE),
        vec![topic0("CommitNewFee(uint256,uint256,uint256)"), uint(1)],
        concat(&[uint(4_000_000), uint(0)]),
    );
    let new_fee = log(&addr(CURVE), vec![topic0("NewFee(uint256,uint256)")], concat(&[uint(3_000_000), uint(0)]));
    assert_eq!(decode(&commit), vec![fee(pb::Protocol::Curvefi, &addr(FACTORY), &addr(CURVE), 4_000_000)]);
    assert_eq!(decode(&new_fee), vec![fee(pb::Protocol::Curvefi, &addr(FACTORY), &addr(CURVE), 3_000_000)]);

    let mut unknown = new_fee.clone();
    unknown.address = addr(UNKNOWN);
    assert!(decode(&unknown).is_empty());
}

fn curve_plain_pool_deployed(fee: Vec<u8>) -> Log {
    log(
        &addr(FACTORY),
        vec![topic0("PlainPoolDeployed(address[],uint256,uint256,address)")],
        concat(&[uint(128), uint(100), fee, word(&addr(0x0d)), uint(2), word(&addr(T0)), word(&addr(T1))]),
    )
}

/// v0.8.0 names the deployed pool after the transaction's first non-reverted
/// call with an address, usually the root call, not the CREATE that
/// `dex-pools` keys the pool by. Kept for parity and listed in the README.
#[test]
fn curvefi_factory_deployments_take_the_first_call_address() {
    let meta = log(
        &addr(FACTORY),
        vec![topic0("MetaPoolDeployed(address,address,uint256,uint256,address)")],
        concat(&[word(&addr(T0)), word(&addr(0xbb)), uint(100), uint(4_000_000), word(&addr(0x0d))]),
    );
    let mut trx = tx();
    trx.calls.push(Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        call_type: CallType::Create.into(),
        address: addr(CURVE),
        ..Default::default()
    });
    for log in [curve_plain_pool_deployed(uint(4_000_000)), meta] {
        assert_eq!(
            decode_log(&trx, &log, &pools()).unwrap(),
            vec![
                init(pb::Protocol::Curvefi, &addr(FACTORY), &addr(ROOT)),
                fee(pb::Protocol::Curvefi, &addr(FACTORY), &addr(ROOT), 4_000_000)
            ]
        );
    }

    // Reverted calls are skipped; without any call the pool is empty.
    trx.calls[0].state_reverted = true;
    assert_eq!(
        decode_log(&trx, &curve_plain_pool_deployed(uint(1)), &pools()).unwrap()[0],
        init(pb::Protocol::Curvefi, &addr(FACTORY), &addr(CURVE))
    );
    trx.calls.clear();
    assert_eq!(
        decode_log(&trx, &curve_plain_pool_deployed(uint(1)), &pools()).unwrap()[0],
        init(pb::Protocol::Curvefi, &addr(FACTORY), &[])
    );
}

/// v0.8.0 converted these fees with `BigInt::to_u64() as u32`: truncated
/// above `u32::MAX`, a panic above `u64::MAX`. The port fails the block with
/// an error where v0.8.0 panicked.
#[test]
fn curvefi_fees_truncate_like_v0_8_0_and_fail_where_it_panicked() {
    let truncated = decode(&curve_plain_pool_deployed(uint(u128::from(u32::MAX) + 2)));
    assert_eq!(truncated[1], fee(pb::Protocol::Curvefi, &addr(FACTORY), &addr(ROOT), 1));

    assert!(decode_log(&tx(), &curve_plain_pool_deployed(uint(u128::from(u64::MAX) + 1)), &pools()).is_err());
    let new_fee = log(&addr(CURVE), vec![topic0("NewFee(uint256,uint256)")], concat(&[vec![0xff; 32], uint(0)]));
    assert!(decode_log(&tx(), &new_fee, &pools()).is_err());
}

// ── pb::Protocol::Balancer ─────────────────────────────────────────────────────────────────────────────

fn balancer_swap(pool: u8) -> Log {
    log(
        &addr(VAULT),
        vec![
            topic0("Swap(address,address,address,uint256,uint256,uint256,uint256)"),
            word(&addr(pool)),
            word(&addr(T1)),
            word(&addr(T0)),
        ],
        concat(&[uint(1000), uint(990), uint(10u128.pow(15)), uint(1)]),
    )
}

#[test]
fn balancer_v3_swaps_name_the_pool_in_the_event() {
    assert_eq!(
        decode(&balancer_swap(BAL)),
        vec![swap(
            pb::Protocol::Balancer,
            &addr(BAL_FACTORY),
            &addr(BAL),
            &addr(TX_FROM),
            (&addr(T1), "1000"),
            (&addr(T0), "990")
        )]
    );
    assert!(decode(&balancer_swap(UNKNOWN)).is_empty());
}

#[test]
fn balancer_v3_pool_registered_initializes_with_the_event_factory() {
    let mut words = vec![uint(21 * 32), uint(10u128.pow(16)), uint(0)];
    words.extend(std::iter::repeat_n(uint(0), 18));
    words.extend([uint(1), word(&addr(T0)), uint(0), word(&addr(0)), uint(0)]);
    let log = log(
        &addr(VAULT),
        vec![
            topic0(
                "PoolRegistered(address,address,(address,uint8,address,bool)[],uint256,uint32,(address,address,address),\
                 (bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,address),(bool,bool,bool,bool))",
            ),
            word(&addr(UNKNOWN)),
            word(&addr(BAL_FACTORY)),
        ],
        concat(&words),
    );
    assert_eq!(decode(&log), vec![init(pb::Protocol::Balancer, &addr(BAL_FACTORY), &addr(UNKNOWN))]);
}

#[test]
fn balancer_fee_changes_convert_18_decimals_to_basis_points() {
    let v2 = log(&addr(BAL), vec![topic0("SwapFeePercentageChanged(uint256)")], uint(3 * 10u128.pow(15)));
    assert_eq!(decode(&v2), vec![fee(pb::Protocol::Balancer, &addr(BAL_FACTORY), &addr(BAL), 30)]);

    let aggregate = |pool: u8| {
        log(
            &addr(VAULT),
            vec![topic0("AggregateSwapFeePercentageChanged(address,uint256)"), word(&addr(pool))],
            uint(10u128.pow(16)),
        )
    };
    assert_eq!(decode(&aggregate(BAL)), vec![fee(pb::Protocol::Balancer, &addr(BAL_FACTORY), &addr(BAL), 100)]);
    assert!(decode(&aggregate(UNKNOWN)).is_empty());
}

// ── pb::Protocol::Bancor ───────────────────────────────────────────────────────────────────────────────

#[test]
fn bancor_conversions_and_fee_updates_of_stored_converters() {
    let conversion = log(
        &addr(BANCOR),
        vec![
            topic0("Conversion(address,address,address,uint256,uint256,int256)"),
            word(&addr(T0)),
            word(&addr(T1)),
            word(&addr(SENDER)),
        ],
        concat(&[uint(1000), uint(990), int(-3)]),
    );
    assert_eq!(
        decode(&conversion),
        vec![swap(
            pb::Protocol::Bancor,
            &addr(FACTORY),
            &addr(BANCOR),
            &addr(SENDER),
            (&addr(T0), "1000"),
            (&addr(T1), "990")
        )]
    );
    let update = log(
        &addr(BANCOR),
        vec![topic0("ConversionFeeUpdate(uint32,uint32)")],
        concat(&[uint(1000), uint(2000)]),
    );
    assert_eq!(decode(&update), vec![fee(pb::Protocol::Bancor, &addr(FACTORY), &addr(BANCOR), 2000)]);

    let mut unknown = conversion.clone();
    unknown.address = addr(UNKNOWN);
    assert!(decode(&unknown).is_empty());
}

#[test]
fn bancor_new_converters_initialize() {
    let new_converter = log(
        &addr(FACTORY),
        vec![
            topic0("NewConverter(uint16,address,address)"),
            uint(3),
            word(&addr(BANCOR)),
            word(&addr(SENDER)),
        ],
        vec![],
    );
    let legacy = log(
        &addr(FACTORY),
        vec![topic0("NewConverter(address,address)"), word(&addr(BANCOR)), word(&addr(SENDER))],
        vec![],
    );
    for log in [new_converter, legacy] {
        assert_eq!(decode(&log), vec![init(pb::Protocol::Bancor, &addr(FACTORY), &addr(BANCOR))]);
    }
}

// ── Aggregators and settlement contracts (no pool lookup) ───────────────────────────────

#[test]
fn cow_trades_use_the_settlement_contract_as_factory_and_pool() {
    let settlement = addr(0xc0);
    let log = log(
        &settlement,
        vec![topic0("Trade(address,address,address,uint256,uint256,uint256,bytes)"), word(&addr(SENDER))],
        concat(&[
            word(&addr(T0)),
            word(&addr(T1)),
            uint(1000),
            uint(990),
            uint(5),
            uint(192),
            uint(56),
            vec![0xab; 64],
        ]),
    );
    assert_eq!(
        decode(&log),
        vec![swap(
            pb::Protocol::Cow,
            &settlement,
            &settlement,
            &addr(SENDER),
            (&addr(T0), "1000"),
            (&addr(T1), "990")
        )]
    );
}

#[test]
fn dodo_order_history_uses_the_proxy_as_factory_and_pool() {
    let proxy = addr(0xd0);
    let log = log(
        &proxy,
        vec![topic0("OrderHistory(address,address,address,uint256,uint256)")],
        concat(&[word(&addr(T0)), word(&addr(T1)), word(&addr(SENDER)), uint(1000), uint(990)]),
    );
    assert_eq!(
        decode(&log),
        vec![swap(pb::Protocol::Dodo, &proxy, &proxy, &addr(SENDER), (&addr(T0), "1000"), (&addr(T1), "990"))]
    );
}

#[test]
fn woofi_swaps_use_the_pool_as_factory_and_report_from() {
    let pp = addr(0xd1);
    let log = log(
        &pp,
        vec![
            topic0("WooSwap(address,address,uint256,uint256,address,address,address,uint256,uint256)"),
            word(&addr(T0)),
            word(&addr(T1)),
            word(&addr(0x70)),
        ],
        concat(&[uint(1000), uint(990), word(&addr(SENDER)), word(&addr(0)), uint(1), uint(2)]),
    );
    assert_eq!(
        decode(&log),
        vec![swap(pb::Protocol::Woofi, &pp, &pp, &addr(SENDER), (&addr(T0), "1000"), (&addr(T1), "990"))]
    );
}

// ── SunPump ──────────────────────────────────────────────────────────────────────────────

fn sunpump_purchase(token: u8) -> Log {
    log(
        &addr(0x1a),
        vec![
            topic0("TokenPurchased(address,address,uint256,uint256,uint256,uint256)"),
            word(&addr(token)),
            word(&addr(SENDER)),
        ],
        concat(&[uint(1000), uint(10), uint(5000), uint(1)]),
    )
}

#[test]
fn sunpump_trades_price_tokens_against_native_trx() {
    let token = addr(SUN_TOKEN);
    assert_eq!(
        decode(&sunpump_purchase(SUN_TOKEN)),
        vec![swap(
            pb::Protocol::Sunpump,
            &addr(FACTORY),
            &token,
            &addr(SENDER),
            (&NULL_ADDRESS, "1000"),
            (&token, "5000")
        )]
    );
    let sold = log(
        &addr(0x1a),
        vec![topic0("TokenSold(address,address,uint256,uint256,uint256)"), word(&token), word(&addr(SENDER))],
        concat(&[uint(1000), uint(10), uint(5000)]),
    );
    assert_eq!(
        decode(&sold),
        vec![swap(
            pb::Protocol::Sunpump,
            &addr(FACTORY),
            &token,
            &addr(SENDER),
            (&token, "5000"),
            (&NULL_ADDRESS, "1000")
        )]
    );
    assert!(decode(&sunpump_purchase(UNKNOWN)).is_empty());
}

#[test]
fn sunpump_token_creations_and_fee_settings() {
    let launchpad = addr(0x1a);
    let v1 = log(
        &launchpad,
        vec![topic0("TokenCreate(address,uint256,address)")],
        concat(&[word(&addr(SUN_TOKEN)), uint(9), word(&addr(SENDER))]),
    );
    let legacy = log(
        &launchpad,
        vec![topic0("TokenCreate(address,address,uint64,uint64,string,string)")],
        concat(&[
            word(&addr(SUN_TOKEN)),
            word(&addr(SENDER)),
            uint(1),
            uint(2),
            uint(192),
            uint(224),
            uint(0),
            uint(0),
        ]),
    );
    for log in [v1, legacy] {
        assert_eq!(decode(&log), vec![init(pb::Protocol::Sunpump, &launchpad, &addr(SUN_TOKEN))]);
    }

    // Fee settings name the launchpad as factory and pool, with no lookup.
    for signature in ["PurchaseFeeSet(uint256,uint256)", "SaleFeeSet(uint256,uint256)"] {
        let set = log(&launchpad, vec![topic0(signature)], concat(&[uint(100), uint(200)]));
        assert_eq!(decode(&set), vec![fee(pb::Protocol::Sunpump, &launchpad, &launchpad, 200)]);
        let overflow = log(&launchpad, vec![topic0(signature)], concat(&[uint(100), vec![0xff; 32]]));
        assert!(decode_log(&tx(), &overflow, &pools()).is_err());
    }
}

// ── project ──────────────────────────────────────────────────────────────────────────────

/// A lookup over `pools()` that records the keys asked for.
#[derive(Default)]
struct Recording(RefCell<Vec<Vec<u8>>>);

impl PoolLookup for Recording {
    fn get_pool(&self, address: &[u8]) -> Option<Pool> {
        self.0.borrow_mut().push(address.to_vec());
        pools().get_pool(address)
    }
}

#[test]
fn project_looks_up_each_emitter_and_the_pools_named_by_v4_balancer_and_sunpump_trades() {
    let input = block(vec![
        trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90]), v2_swap(V2, [0, 100, 90, 0])]),
        trx(&addr(ROOT), vec![v4_swap(&v4_id(), 100, -90), balancer_swap(BAL), sunpump_purchase(SUN_TOKEN)]),
        trx(&addr(ROOT), vec![v4_initialize(&[0x46; 32])]),
    ]);
    let lookup = Recording::default();
    project(&input, &lookup).unwrap();

    let mut asked = lookup.0.into_inner();
    asked.sort();
    let mut expected = vec![addr(V2), addr(POOL_MANAGER), v4_id(), addr(VAULT), addr(BAL), addr(0x1a), addr(SUN_TOKEN)];
    expected.sort();
    assert_eq!(asked, expected, "each key once; the V4 Initialize id is not looked up");
}

/// v0.8.0 looks up a V4 `Initialize` pool id only when a V4 swap in the same
/// block names it, so the factory is usually empty. Kept for parity.
#[test]
fn project_v4_initialize_factory_is_empty_unless_a_swap_names_the_pool() {
    let initialize_only = block(vec![trx(&addr(ROOT), vec![v4_initialize(&v4_id())])]);
    let events = project(&initialize_only, &pools()).unwrap();
    assert_eq!(events.transactions[0].logs[0].log, Some(init(pb::Protocol::UniswapV4, &[], &v4_id())));

    let with_swap = block(vec![trx(&addr(ROOT), vec![v4_initialize(&v4_id()), v4_swap(&v4_id(), 100, -90)])]);
    let events = project(&with_swap, &pools()).unwrap();
    assert_eq!(
        events.transactions[0].logs[0].log,
        Some(init(pb::Protocol::UniswapV4, &addr(POOL_MANAGER), &v4_id()))
    );
}

#[test]
fn project_copies_transaction_log_and_call_metadata() {
    let mut trace = trx(&addr(ROOT), vec![log(&addr(0x01), vec![], vec![]), v2_swap(V2, [100, 0, 0, 90])]);
    trace.nonce = 7;
    trace.gas_limit = 50_000;
    trace.receipt.as_option_mut().unwrap().cumulative_gas_used = 123_456;
    trace.calls[0].logs[1].block_index = 42;
    trace.calls[0].gas_consumed = 21_000;
    let source_log = trace.calls[0].logs[1].clone();

    let events = project(&block(vec![trace]), &pools()).unwrap();

    assert_eq!(events.transactions.len(), 1);
    let transaction = &events.transactions[0];
    assert_eq!(transaction.hash, vec![0xaa; 32]);
    assert_eq!(transaction.from, addr(TX_FROM));
    assert_eq!(transaction.to, Some(addr(ROOT)));
    assert_eq!((transaction.nonce, transaction.gas_limit, transaction.gas_used), (7, 50_000, 123_456));
    assert_eq!((transaction.gas_price.as_str(), transaction.value.as_str()), ("0", "0"));

    // Only the decoded log is kept, with its raw fields and emitting call.
    assert_eq!(transaction.logs.len(), 1);
    let out = &transaction.logs[0];
    assert_eq!(
        (&out.address, out.ordinal, &out.topics, &out.data, out.block_index),
        (&source_log.address, 11, &source_log.topics, &source_log.data, 42)
    );
    let call = out.call.as_option().unwrap();
    assert_eq!((call.index, call.address.clone(), call.gas_consumed), (1, addr(ROOT), 21_000));
    assert_eq!(call.call_type, pb::CallType::Call);
    assert_eq!(
        out.log,
        Some(swap(
            pb::Protocol::UniswapV2,
            &addr(FACTORY),
            &addr(V2),
            &addr(TX_FROM),
            (&addr(T0), "100"),
            (&addr(T1), "90")
        ))
    );

    // The output survives a protobuf round trip unchanged.
    assert_eq!(pb::Events::decode_from_slice(&events.encode_to_vec()).unwrap(), events);
}

#[test]
fn project_leaves_out_failed_transactions_and_transactions_without_events() {
    let mut failed = trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90])]);
    failed.status = TransactionTraceStatus::Failed.into();
    failed.receipt = buffa::MessageField::none();
    let unrelated = trx(&addr(ROOT), vec![v2_swap(UNKNOWN, [100, 0, 0, 90])]);

    assert_eq!(project(&block(vec![failed, unrelated]), &pools()).unwrap(), pb::Events::default());
    assert_eq!(project(&block(vec![]), &pools()).unwrap(), pb::Events::default());
}

#[test]
fn project_reads_receipt_logs_without_call_traces_and_skips_reverted_calls() {
    let mut receipt_only = trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90])]);
    receipt_only.calls.clear();
    let events = project(&block(vec![receipt_only]), &pools()).unwrap();
    assert_eq!(events.transactions[0].logs.len(), 1);
    assert!(events.transactions[0].logs[0].call.is_unset());

    let mut reverted = trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90])]);
    reverted.calls[0].state_reverted = true;
    assert!(project(&block(vec![reverted]), &pools()).unwrap().transactions.is_empty());
}

#[test]
fn project_fails_where_v0_8_0_panicked() {
    // A successful transaction without a receipt, even with call traces.
    let mut no_receipt = trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90])]);
    no_receipt.receipt = buffa::MessageField::none();
    assert!(project(&block(vec![no_receipt]), &pools()).is_err());

    // A Curve deployment fee above u64::MAX.
    let overflow = trx(&addr(ROOT), vec![curve_plain_pool_deployed(vec![0xff; 32])]);
    assert!(project(&block(vec![overflow]), &pools()).is_err());
}
