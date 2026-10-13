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

const VAULT: u8 = 0xba;
const POOL: u8 = 0x11;

fn swap_log() -> Log {
    log(
        &addr(VAULT),
        vec![
            topic0("Swap(address,address,address,uint256,uint256,uint256,uint256)"),
            word(&addr(POOL)),
            word(&addr(0xa1)),
            word(&addr(0xa2)),
        ],
        [uint(100), uint(99), uint(3_000_000_000_000_000), uint(300)].concat(),
    )
}

fn liquidity_log(signature: &str, kind: Vec<u8>) -> Log {
    log(
        &addr(VAULT),
        vec![topic0(signature), word(&addr(POOL)), word(&addr(0xa1)), kind],
        encode(&[
            Abi::Word(uint(1_000)),
            Abi::Array(vec![Abi::Word(uint(1)), Abi::Word(uint(2))]),
            Abi::Array(vec![Abi::Word(uint(3)), Abi::Word(uint(4))]),
        ]),
    )
}

common::transaction_tests!(swap_log());

#[test]
fn v2_swap_fee_percentage_changed() {
    let log = log(&addr(POOL), vec![topic0("SwapFeePercentageChanged(uint256)")], uint(3_000_000_000_000_000));
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::SwapFeePercentage {
            swap_fee_percentage: "3000000000000000".into(),
        })
    );
}

#[test]
fn v2_paused_state_changed() {
    let log = log(&addr(POOL), vec![topic0("PausedStateChanged(bool)")], boolean(true));
    assert_eq!(decode(log), pb::log::Log::from(pb::Paused { paused: true }));
}

#[test]
fn v2_recovery_mode_state_changed() {
    let log = log(&addr(POOL), vec![topic0("RecoveryModeStateChanged(bool)")], boolean(true));
    assert_eq!(decode(log), pb::log::Log::from(pb::RecoveryMode { enabled: true }));
}

#[test]
fn v2_protocol_fee_percentage_cache_updated() {
    let log = log(
        &addr(POOL),
        vec![topic0("ProtocolFeePercentageCacheUpdated(uint256,uint256)"), uint(2)],
        uint(500),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::ProtocolFeePercentage {
            fee_type: "2".into(),
            protocol_fee_percentage: "500".into(),
        })
    );
}

#[test]
fn v3_amp_update_started() {
    let log = log(
        &addr(POOL),
        vec![topic0("AmpUpdateStarted(uint256,uint256,uint256,uint256)")],
        [uint(100), uint(200), uint(1_700_000_000), uint(1_700_086_400)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::AmpUpdateStarted {
            start_value: "100".into(),
            end_value: "200".into(),
            start_time: "1700000000".into(),
            end_time: "1700086400".into(),
        })
    );
}

#[test]
fn v3_amp_update_stopped() {
    let log = log(&addr(POOL), vec![topic0("AmpUpdateStopped(uint256)")], uint(150));
    assert_eq!(decode(log), pb::log::Log::from(pb::AmpUpdateStopped { current_value: "150".into() }));
}

#[test]
fn v3_vault_swap() {
    assert_eq!(
        decode(swap_log()),
        pb::log::Log::from(pb::VaultSwap {
            pool: addr(POOL),
            token_in: addr(0xa1),
            token_out: addr(0xa2),
            amount_in: "100".into(),
            amount_out: "99".into(),
            swap_fee_percentage: "3000000000000000".into(),
            swap_fee_amount: "300".into(),
        })
    );
}

#[test]
fn v3_liquidity_added() {
    let log = liquidity_log("LiquidityAdded(address,address,uint8,uint256,uint256[],uint256[])", uint(1));
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::LiquidityAdded {
            pool: addr(POOL),
            liquidity_provider: addr(0xa1),
            kind: 1,
            total_supply: "1000".into(),
            amounts_added_raw: vec!["1".into(), "2".into()],
            swap_fee_amounts_raw: vec!["3".into(), "4".into()],
        })
    );
}

#[test]
fn v3_liquidity_removed() {
    let log = liquidity_log("LiquidityRemoved(address,address,uint8,uint256,uint256[],uint256[])", uint(2));
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::LiquidityRemoved {
            pool: addr(POOL),
            liquidity_provider: addr(0xa1),
            kind: 2,
            total_supply: "1000".into(),
            amounts_removed_raw: vec!["1".into(), "2".into()],
            swap_fee_amounts_raw: vec!["3".into(), "4".into()],
        })
    );
}

#[test]
fn a_wide_liquidity_kind_saturates_at_u32_max_and_fails_above_u64_max() {
    // v0.8.0: `kind.to_u64().min(u32::MAX)`, so 2^40 saturates...
    let log = liquidity_log("LiquidityAdded(address,address,uint8,uint256,uint256[],uint256[])", uint(1 << 40));
    let pb::log::Log::LiquidityAdded(event) = decode(log) else {
        panic!("LiquidityAdded")
    };
    assert_eq!(event.kind, u32::MAX);

    // ...and 2^64 made `to_u64()` panic; the port fails the block instead.
    let log = liquidity_log("LiquidityAdded(address,address,uint8,uint256,uint256[],uint256[])", uint(1 << 64));
    let error = project(&block(vec![trx(&addr(0x01), vec![log])])).unwrap_err();
    assert_eq!(
        error.to_string(),
        "LiquidityAdded.kind: BigInt '18446744073709551616' is too large to fit into u64"
    );
}

#[test]
fn v3_pool_initialized() {
    let log = log(&addr(VAULT), vec![topic0("PoolInitialized(address)"), word(&addr(POOL))], vec![]);
    assert_eq!(decode(log), pb::log::Log::from(pb::PoolInitialized { pool: addr(POOL) }));
}

#[test]
fn v3_pool_registered() {
    let token_config = |token: u8, token_type: u128, rate_provider: u8, pays_yield_fees: bool| {
        Abi::Tuple(vec![
            Abi::Word(word(&addr(token))),
            Abi::Word(uint(token_type)),
            Abi::Word(word(&addr(rate_provider))),
            Abi::Word(boolean(pays_yield_fees)),
        ])
    };
    let flags = |values: &[bool]| values.iter().map(|value| Abi::Word(boolean(*value))).collect::<Vec<_>>();
    let mut hooks = flags(&[true, false, true, false, true, false, true, false, true, false]);
    hooks.push(Abi::Word(word(&addr(0xd1))));
    let data = encode(&[
        Abi::Array(vec![token_config(0xc1, 0, 0x00, false), token_config(0xc2, 1, 0xc3, true)]),
        Abi::Word(uint(1_000_000_000_000_000)),
        Abi::Word(uint(1_800_000_000)),
        Abi::Tuple(vec![Abi::Word(word(&addr(0xe1))), Abi::Word(word(&addr(0xe2))), Abi::Word(word(&addr(0xe3)))]),
        Abi::Tuple(hooks),
        Abi::Tuple(flags(&[false, true, false, true])),
    ]);
    let log = log(
        &addr(VAULT),
        vec![
            topic0(
                "PoolRegistered(address,address,(address,uint8,address,bool)[],uint256,uint32,(address,address,address),\
                 (bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,address),(bool,bool,bool,bool))",
            ),
            word(&addr(POOL)),
            word(&addr(0xf1)),
        ],
        data,
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::PoolRegistered {
            pool: addr(POOL),
            factory: addr(0xf1),
            token_config: vec![
                pb::TokenConfig {
                    token: addr(0xc1),
                    token_type: 0,
                    rate_provider: addr(0x00),
                    paysYieldFees: false,
                },
                pb::TokenConfig {
                    token: addr(0xc2),
                    token_type: 1,
                    rate_provider: addr(0xc3),
                    paysYieldFees: true,
                },
            ],
            swap_fee_percentage: "1000000000000000".into(),
            pause_window_end_time: "1800000000".into(),
            role_accounts: pb::RoleAccounts {
                pause_manager: addr(0xe1),
                swap_fee_manager: addr(0xe2),
                pool_creator: addr(0xe3),
            }
            .into(),
            hooks_config: pb::HooksConfig {
                enable_hook_adjusted_amounts: true,
                should_call_before_initialize: false,
                should_call_after_initialize: true,
                should_call_compute_dynamic_swap_fee: false,
                should_call_before_swap: true,
                should_call_after_swap: false,
                should_call_before_add_liquidity: true,
                should_call_after_add_liquidity: false,
                should_call_before_remove_liquidity: true,
                should_call_after_remove_liquidity: false,
                hooks_address: addr(0xd1),
            }
            .into(),
            liquidity_management: pb::LiquidityManagement {
                disable_unbalanced_liquidity: false,
                enable_add_liquidity_custom: true,
                enable_remove_liquidity_custom: false,
                enable_donation: true,
            }
            .into(),
        })
    );
}

#[test]
fn v3_pool_paused_state_changed() {
    let log = log(
        &addr(VAULT),
        vec![topic0("PoolPausedStateChanged(address,bool)"), word(&addr(POOL))],
        boolean(true),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::PoolPaused {
            pool: addr(POOL),
            paused: true,
        })
    );
}

#[test]
fn v3_pool_recovery_mode_state_changed() {
    let log = log(
        &addr(VAULT),
        vec![topic0("PoolRecoveryModeStateChanged(address,bool)"), word(&addr(POOL))],
        boolean(true),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::PoolRecoveryMode {
            pool: addr(POOL),
            enabled: true,
        })
    );
}

#[test]
fn v3_aggregate_swap_fee_percentage_changed() {
    let log = log(
        &addr(VAULT),
        vec![topic0("AggregateSwapFeePercentageChanged(address,uint256)"), word(&addr(POOL))],
        uint(250_000_000_000_000),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::AggregateSwapFeePercentage {
            pool: addr(POOL),
            aggregate_swap_fee_percentage: "250000000000000".into(),
        })
    );
}
