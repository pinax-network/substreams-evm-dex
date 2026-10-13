//! The first seven tests are v0.8.0's (`dex/curvefi/src/lib.rs`, `9663b25`),
//! adapted to buffa; the rest decode one synthetic log per event type, with
//! topic 0 the Keccak-256 of the canonical signature from the substreams-abis
//! ABI JSON.

use super::*;
use common::testing::{addr, block, encode, int, log, topic0, trx, uint, word, Abi};
use std::str::FromStr;
use substreams::scalar::BigInt;
use substreams::Hex;
use substreams_ethereum::pb::eth::v2::{TransactionReceipt, TransactionTraceStatus};

fn sample_stableswap_constructor() -> curvefi::stableswap::constructor::Constructor {
    curvefi::stableswap::constructor::Constructor {
        owner: vec![0x11; 20],
        coins: [vec![0x21; 20], vec![0x22; 20], vec![0x23; 20]],
        pool_token: vec![0x31; 20],
        a: BigInt::from(2000u64),
        fee: BigInt::from(4_000_000u64),
        admin_fee: BigInt::from(5_000_000_000u64),
    }
}

#[test]
fn extracts_stableswap_init_from_create_input_tail() {
    let constructor = sample_stableswap_constructor();
    let constructor_input = constructor.encode();
    let mut transaction_input = vec![0x60, 0x60, 0x60, 0x40, 0x52];
    transaction_input.extend(constructor_input.clone());

    let trx = TransactionTrace {
        input: transaction_input,
        calls: vec![Call {
            call_type: CallType::Create.into(),
            depth: 0,
            address: vec![0xaa; 20],
            begin_ordinal: 42,
            input: vec![],
            ..Default::default()
        }],
        ..Default::default()
    };

    let (init, create_call) = try_extract_pool_init(&trx).expect("expected init event");

    assert_eq!(trx.input[..5], [0x60, 0x60, 0x60, 0x40, 0x52]);
    assert_eq!(trx.input[5..], constructor_input);
    assert_eq!(init.address, vec![0xaa; 20]);
    assert_eq!(init.owner, constructor.owner);
    assert_eq!(init.coins, constructor.coins.to_vec());
    assert_eq!(init.pool_token, constructor.pool_token);
    assert_eq!(init.a, "2000");
    assert_eq!(init.fee, "4000000");
    assert_eq!(init.admin_fee, "5000000000");
    assert_eq!(create_call.begin_ordinal, 42);
}

#[test]
fn ignores_non_contract_creation_transactions() {
    let trx = TransactionTrace {
        to: vec![0xbb; 20],
        calls: vec![Call {
            call_type: CallType::Create.into(),
            depth: 0,
            input: sample_stableswap_constructor().encode(),
            ..Default::default()
        }],
        ..Default::default()
    };

    assert!(try_extract_pool_init(&trx).is_none());
}

#[test]
fn treats_zero_address_to_as_contract_creation() {
    let trx = TransactionTrace {
        to: vec![0x00; 20],
        ..Default::default()
    };

    assert!(is_contract_creation_transaction(&trx));
}

#[test]
fn treats_root_create_call_as_contract_creation() {
    let trx = TransactionTrace {
        to: vec![0x12; 20],
        calls: vec![Call {
            call_type: CallType::Create.into(),
            depth: 0,
            ..Default::default()
        }],
        ..Default::default()
    };

    assert!(is_contract_creation_transaction(&trx));
}

#[test]
fn ignores_create_calls_without_stableswap_constructor_tail() {
    let trx = TransactionTrace {
        input: vec![0x01, 0x02, 0x03],
        calls: vec![Call {
            call_type: CallType::Create.into(),
            depth: 0,
            input: vec![],
            ..Default::default()
        }],
        ..Default::default()
    };

    assert!(try_extract_pool_init(&trx).is_none());
}

#[test]
fn extracts_stableswap_init_from_real_deployment_input() {
    let constructor = curvefi::stableswap::constructor::Constructor {
        owner: Hex::decode("6e8f6d1da6232d5e40b0b8758a0145d6c5123eb7").unwrap(),
        coins: [
            Hex::decode("6b175474e89094c44da98b954eedeac495271d0f").unwrap(),
            Hex::decode("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            Hex::decode("dac17f958d2ee523a2206206994597c13d831ec7").unwrap(),
        ],
        pool_token: Hex::decode("6c3f90f043a72fa612cbac8115ee7e52bde6e490").unwrap(),
        a: BigInt::from_str("100").unwrap(),
        fee: BigInt::from_str("4000000").unwrap(),
        admin_fee: BigInt::from_str("0").unwrap(),
    };
    let mut input = vec![0x74; 2048];
    input.extend(constructor.encode());

    let trx = TransactionTrace {
        input,
        calls: vec![Call {
            call_type: CallType::Create.into(),
            depth: 0,
            address: Hex::decode("6c3f90f043a72fa612cbac8115ee7e52bde6e490").unwrap(),
            begin_ordinal: 42,
            ..Default::default()
        }],
        ..Default::default()
    };

    let (init, create_call) = try_extract_pool_init(&trx).expect("expected init event");

    assert_eq!(init.owner, Hex::decode("6e8f6d1da6232d5e40b0b8758a0145d6c5123eb7").unwrap());
    assert_eq!(
        init.coins,
        vec![
            Hex::decode("6b175474e89094c44da98b954eedeac495271d0f").unwrap(),
            Hex::decode("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            Hex::decode("dac17f958d2ee523a2206206994597c13d831ec7").unwrap(),
        ]
    );
    assert_eq!(init.pool_token, Hex::decode("6c3f90f043a72fa612cbac8115ee7e52bde6e490").unwrap());
    assert_eq!(init.a, BigInt::from_str("100").unwrap().to_string());
    assert_eq!(init.fee, BigInt::from_str("4000000").unwrap().to_string());
    assert_eq!(init.admin_fee, BigInt::from_str("0").unwrap().to_string());
    assert_eq!(create_call.begin_ordinal, 42);
}

/// Integration test: verifies that `project` emits exactly one synthetic `Init` log for a
/// block containing only a direct CurveFi 3pool deployment transaction
/// (`0x20793bbf260912aae189d5d261ff003c9b9166da8191d8f9d63ff1c7722f3ac6`, block 10809473).
///
/// This test proves that the emission logic is correct and that the absence of the `Init`
/// event when running the live CLI is an output-visibility issue, not an emission bug.
#[test]
fn map_events_emits_synthetic_init_for_direct_pool_deployment() {
    // Known constructor values from the real 3pool deployment transaction.
    let constructor = curvefi::stableswap::constructor::Constructor {
        owner: Hex::decode("6e8f6d1da6232d5e40b0b8758a0145d6c5123eb7").unwrap(),
        coins: [
            Hex::decode("6b175474e89094c44da98b954eedeac495271d0f").unwrap(), // DAI
            Hex::decode("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(), // USDC
            Hex::decode("dac17f958d2ee523a2206206994597c13d831ec7").unwrap(), // USDT
        ],
        pool_token: Hex::decode("6c3f90f043a72fa612cbac8115ee7e52bde6e490").unwrap(),
        a: BigInt::from_str("100").unwrap(),
        fee: BigInt::from_str("4000000").unwrap(),
        admin_fee: BigInt::from_str("0").unwrap(),
    };

    // Simulate the transaction input: arbitrary deployment bytecode followed by the
    // ABI-encoded constructor tail (last 256 bytes).
    let mut tx_input = vec![0x60u8; 2048];
    tx_input.extend(constructor.encode());

    let trx = TransactionTrace {
        hash: Hex::decode("20793bbf260912aae189d5d261ff003c9b9166da8191d8f9d63ff1c7722f3ac6").unwrap(),
        from: Hex::decode("babe61887f1de2713c6f97e567623453d3c79f67").unwrap(),
        // to is empty – this is a contract-creation transaction
        to: vec![],
        input: tx_input,
        // The transaction succeeded (required by block.transactions())
        status: TransactionTraceStatus::Succeeded.into(),
        calls: vec![Call {
            call_type: CallType::Create.into(),
            depth: 0,
            address: Hex::decode("bebc44782c7db0a1a60cb6fe97d0b483032ff1c7").unwrap(),
            begin_ordinal: 100,
            ..Default::default()
        }],
        receipt: TransactionReceipt::default().into(),
        ..Default::default()
    };

    let block = Block {
        transaction_traces: vec![trx],
        ..Default::default()
    };

    let events = project(&block).expect("project must not fail");

    // Exactly one transaction should be emitted (the deployment tx has a synthetic log).
    assert_eq!(events.transactions.len(), 1, "expected exactly one transaction in output");

    let transaction = &events.transactions[0];
    assert_eq!(
        transaction.hash,
        Hex::decode("20793bbf260912aae189d5d261ff003c9b9166da8191d8f9d63ff1c7722f3ac6").unwrap(),
    );

    // Exactly one log: the synthetic Init.
    assert_eq!(transaction.logs.len(), 1, "expected exactly one synthetic Init log");
    let log = &transaction.logs[0];

    // The synthetic log address must be the deployed 3pool contract address.
    assert_eq!(
        log.address,
        Hex::decode("bebc44782c7db0a1a60cb6fe97d0b483032ff1c7").unwrap(),
        "synthetic log address must match the created contract",
    );

    // Extract and validate the Init payload.
    let init = match log.log.as_ref().expect("log event must be present") {
        pb::log::Log::Init(init) => init,
        other => panic!("expected pb::log::Log::Init, got {:?}", other),
    };

    assert_eq!(init.address, Hex::decode("bebc44782c7db0a1a60cb6fe97d0b483032ff1c7").unwrap());
    assert_eq!(init.owner, Hex::decode("6e8f6d1da6232d5e40b0b8758a0145d6c5123eb7").unwrap());
    assert_eq!(
        init.coins,
        vec![
            Hex::decode("6b175474e89094c44da98b954eedeac495271d0f").unwrap(),
            Hex::decode("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            Hex::decode("dac17f958d2ee523a2206206994597c13d831ec7").unwrap(),
        ],
    );
    assert_eq!(init.pool_token, Hex::decode("6c3f90f043a72fa612cbac8115ee7e52bde6e490").unwrap());
    assert_eq!(init.a, "100");
    assert_eq!(init.fee, "4000000");
    assert_eq!(init.admin_fee, "0");

    // Verify the synthetic log carries the CREATE-call metadata.
    let call_meta = log.call.as_option().expect("synthetic log must carry call metadata");
    assert_eq!(
        call_meta.address,
        Hex::decode("bebc44782c7db0a1a60cb6fe97d0b483032ff1c7").unwrap(),
        "call metadata address must match the created contract",
    );
    assert_eq!(call_meta.begin_ordinal, 100, "call metadata begin_ordinal must match the CREATE call");
}

// ── One synthetic log per event type ────────────────────────────────────────

/// The event `project` decodes from a block holding only `log`, emitted by a
/// plain call (no `Init`).
fn decode(log: Log) -> pb::log::Log {
    let events = project(&block(vec![trx(&addr(0x01), vec![log])])).expect("project");
    let [transaction] = events.transactions.as_slice() else {
        panic!("one transaction")
    };
    let [entry] = transaction.logs.as_slice() else { panic!("one decoded log") };
    entry.log.clone().expect("event")
}

const POOL: u8 = 0x11;

fn amounts(values: &[u128]) -> Abi {
    Abi::Tuple(values.iter().map(|value| Abi::Word(uint(*value))).collect())
}

fn strings(values: &[u128]) -> Vec<String> {
    values.iter().map(u128::to_string).collect()
}

fn token_exchange_log() -> Log {
    log(
        &addr(POOL),
        vec![topic0("TokenExchange(address,int128,uint256,int128,uint256)"), word(&addr(0xa1))],
        [int(0), uint(1_000), int(2), uint(998)].concat(),
    )
}

common::transaction_tests!(token_exchange_log());

#[test]
fn pool_token_exchange() {
    assert_eq!(
        decode(token_exchange_log()),
        pb::log::Log::from(pb::TokenExchange {
            buyer: addr(0xa1),
            sold_id: "0".into(),
            tokens_sold: "1000".into(),
            bought_id: "2".into(),
            tokens_bought: "998".into(),
        })
    );
}

#[test]
fn pool_add_liquidity() {
    let log = log(
        &addr(POOL),
        vec![topic0("AddLiquidity(address,uint256[3],uint256[3],uint256,uint256)"), word(&addr(0xa1))],
        encode(&[amounts(&[1, 2, 3]), amounts(&[4, 5, 6]), Abi::Word(uint(7)), Abi::Word(uint(8))]),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::AddLiquidity {
            provider: addr(0xa1),
            token_amounts: strings(&[1, 2, 3]),
            fees: strings(&[4, 5, 6]),
            invariant: "7".into(),
            token_supply: "8".into(),
        })
    );
}

#[test]
fn pool_remove_liquidity() {
    let log = log(
        &addr(POOL),
        vec![topic0("RemoveLiquidity(address,uint256[3],uint256[3],uint256)"), word(&addr(0xa1))],
        encode(&[amounts(&[1, 2, 3]), amounts(&[4, 5, 6]), Abi::Word(uint(9))]),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::RemoveLiquidity {
            provider: addr(0xa1),
            token_amounts: strings(&[1, 2, 3]),
            fees: strings(&[4, 5, 6]),
            token_supply: "9".into(),
        })
    );
}

#[test]
fn pool_remove_liquidity_one() {
    let log = log(
        &addr(POOL),
        vec![topic0("RemoveLiquidityOne(address,uint256,uint256)"), word(&addr(0xa1))],
        [uint(10), uint(11)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::RemoveLiquidityOne {
            provider: addr(0xa1),
            token_amount: "10".into(),
            coin_amount: "11".into(),
        })
    );
}

#[test]
fn pool_remove_liquidity_imbalance() {
    let log = log(
        &addr(POOL),
        vec![
            topic0("RemoveLiquidityImbalance(address,uint256[3],uint256[3],uint256,uint256)"),
            word(&addr(0xa1)),
        ],
        encode(&[amounts(&[1, 2, 3]), amounts(&[4, 5, 6]), Abi::Word(uint(12)), Abi::Word(uint(13))]),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::RemoveLiquidityImbalance {
            provider: addr(0xa1),
            token_amounts: strings(&[1, 2, 3]),
            fees: strings(&[4, 5, 6]),
            invariant: "12".into(),
            token_supply: "13".into(),
        })
    );
}

#[test]
fn pool_admin_and_fee_changes() {
    let log_of = |topics: Vec<Vec<u8>>, data: Vec<u8>| log(&addr(POOL), topics, data);
    assert_eq!(
        decode(log_of(
            vec![topic0("CommitNewAdmin(uint256,address)"), uint(1_700_000_000), word(&addr(0xb1))],
            vec![]
        )),
        pb::log::Log::from(pb::CommitNewAdmin {
            deadline: "1700000000".into(),
            admin: addr(0xb1),
        })
    );
    assert_eq!(
        decode(log_of(vec![topic0("NewAdmin(address)"), word(&addr(0xb2))], vec![])),
        pb::log::Log::from(pb::NewAdmin { admin: addr(0xb2) })
    );
    assert_eq!(
        decode(log_of(
            vec![topic0("CommitNewFee(uint256,uint256,uint256)"), uint(1_700_000_001)],
            [uint(4_000_000), uint(5_000_000_000)].concat()
        )),
        pb::log::Log::from(pb::CommitNewFee {
            deadline: "1700000001".into(),
            fee: "4000000".into(),
            admin_fee: "5000000000".into(),
        })
    );
    assert_eq!(
        decode(log_of(vec![topic0("NewFee(uint256,uint256)")], [uint(3_000_000), uint(0)].concat())),
        pb::log::Log::from(pb::NewFee {
            fee: "3000000".into(),
            admin_fee: "0".into(),
        })
    );
}

#[test]
fn pool_ramp_a() {
    let ramp = log(
        &addr(POOL),
        vec![topic0("RampA(uint256,uint256,uint256,uint256)")],
        [uint(100), uint(200), uint(1), uint(2)].concat(),
    );
    assert_eq!(
        decode(ramp),
        pb::log::Log::from(pb::RampA {
            old_a: "100".into(),
            new_a: "200".into(),
            initial_time: "1".into(),
            future_time: "2".into(),
        })
    );
    let stop = log(&addr(POOL), vec![topic0("StopRampA(uint256,uint256)")], [uint(150), uint(3)].concat());
    assert_eq!(
        decode(stop),
        pb::log::Log::from(pb::StopRampA {
            a: "150".into(),
            t: "3".into()
        })
    );
}

/// A factory deployment transaction: the root call CREATEs the pool at
/// `0xc0` and emits `log`.
fn deployment(log: Log) -> pb::log::Log {
    let mut deployment = trx(&addr(0x0f), vec![log]);
    deployment.calls[0].call_type = CallType::Create.into();
    deployment.calls[0].address = addr(0xc0);
    let events = project(&block(vec![deployment])).expect("project");
    let [transaction] = events.transactions.as_slice() else {
        panic!("one transaction")
    };
    let [entry] = transaction.logs.as_slice() else { panic!("one decoded log") };
    entry.log.clone().expect("event")
}

fn plain_pool_deployed_log() -> Log {
    log(
        &addr(0x0f),
        vec![topic0("PlainPoolDeployed(address[],uint256,uint256,address)")],
        encode(&[
            Abi::Array(vec![Abi::Word(word(&addr(0xd1))), Abi::Word(word(&addr(0xd2)))]),
            Abi::Word(uint(200)),
            Abi::Word(uint(4_000_000)),
            Abi::Word(word(&addr(0xd3))),
        ]),
    )
}

#[test]
fn factory_plain_pool_deployed_names_the_created_pool() {
    assert_eq!(
        deployment(plain_pool_deployed_log()),
        pb::log::Log::from(pb::PlainPoolDeployed {
            address: addr(0xc0),
            coins: vec![addr(0xd1), addr(0xd2)],
            A: "200".into(),
            fee: "4000000".into(),
            deployer: addr(0xd3),
        })
    );
}

#[test]
fn factory_deployment_without_a_create_call_is_dropped() {
    // v0.8.0: the event matched (so no later decoder ran) but, without a
    // CREATE call to name the pool, nothing was emitted.
    let events = project(&block(vec![trx(&addr(0x0f), vec![plain_pool_deployed_log()])])).expect("project");
    assert!(events.transactions.is_empty());
}

#[test]
fn factory_meta_pool_deployed() {
    let log = log(
        &addr(0x0f),
        vec![topic0("MetaPoolDeployed(address,address,uint256,uint256,address)")],
        [word(&addr(0xd1)), word(&addr(0xd4)), uint(100), uint(4_000_000), word(&addr(0xd3))].concat(),
    );
    assert_eq!(
        deployment(log),
        pb::log::Log::from(pb::MetaPoolDeployed {
            address: addr(0xc0),
            coin: addr(0xd1),
            base_pool: addr(0xd4),
            A: "100".into(),
            fee: "4000000".into(),
            deployer: addr(0xd3),
        })
    );
}

#[test]
fn factory_base_pool_added_and_gauge_deployed() {
    let log_of = |signature: &str, data: Vec<u8>| log(&addr(0x0f), vec![topic0(signature)], data);
    assert_eq!(
        decode(log_of("BasePoolAdded(address)", word(&addr(0xd4)))),
        pb::log::Log::from(pb::BasePoolAdded { base_pool: addr(0xd4) })
    );
    assert_eq!(
        decode(log_of(
            "LiquidityGaugeDeployed(address,address)",
            [word(&addr(0xd5)), word(&addr(0xd6))].concat()
        )),
        pb::log::Log::from(pb::LiquidityGaugeDeployed {
            pool: addr(0xd5),
            gauge: addr(0xd6),
        })
    );
}

#[test]
fn cryptoswap_token_exchange() {
    let log = log(
        &addr(POOL),
        vec![topic0("TokenExchange(address,uint256,uint256,uint256,uint256)"), word(&addr(0xa1))],
        [uint(1), uint(1_000), uint(0), uint(997)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::CryptoSwapTokenExchange {
            buyer: addr(0xa1),
            sold_id: "1".into(),
            tokens_sold: "1000".into(),
            bought_id: "0".into(),
            tokens_bought: "997".into(),
        })
    );
}

#[test]
fn cryptoswap_liquidity() {
    let log_of = |signature: &str, data: Vec<u8>| log(&addr(POOL), vec![topic0(signature), word(&addr(0xa1))], data);
    assert_eq!(
        decode(log_of(
            "AddLiquidity(address,uint256[3],uint256,uint256)",
            encode(&[amounts(&[1, 2, 3]), Abi::Word(uint(4)), Abi::Word(uint(5))])
        )),
        pb::log::Log::from(pb::CryptoSwapAddLiquidity {
            provider: addr(0xa1),
            token_amounts: strings(&[1, 2, 3]),
            fee: "4".into(),
            token_supply: "5".into(),
        })
    );
    assert_eq!(
        decode(log_of(
            "RemoveLiquidity(address,uint256[3],uint256)",
            encode(&[amounts(&[1, 2, 3]), Abi::Word(uint(6))])
        )),
        pb::log::Log::from(pb::CryptoSwapRemoveLiquidity {
            provider: addr(0xa1),
            token_amounts: strings(&[1, 2, 3]),
            token_supply: "6".into(),
        })
    );
    assert_eq!(
        decode(log_of(
            "RemoveLiquidityOne(address,uint256,uint256,uint256)",
            [uint(7), uint(1), uint(8)].concat()
        )),
        pb::log::Log::from(pb::CryptoSwapRemoveLiquidityOne {
            provider: addr(0xa1),
            token_amount: "7".into(),
            coin_index: "1".into(),
            coin_amount: "8".into(),
        })
    );
    assert_eq!(
        decode(log_of("ClaimAdminFee(address,uint256)", uint(9))),
        pb::log::Log::from(pb::CryptoSwapClaimAdminFee {
            admin: addr(0xa1),
            tokens: "9".into(),
        })
    );
}

#[test]
fn cryptoswap_parameters() {
    let log_of = |topics: Vec<Vec<u8>>, words: &[u128]| log(&addr(POOL), topics, words.iter().map(|value| uint(*value)).collect::<Vec<_>>().concat());
    assert_eq!(
        decode(log_of(
            vec![
                topic0("CommitNewParameters(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256)"),
                uint(1_700_000_000)
            ],
            &[1, 2, 3, 4, 5, 6, 7]
        )),
        pb::log::Log::from(pb::CryptoSwapCommitNewParameters {
            deadline: "1700000000".into(),
            admin_fee: "1".into(),
            mid_fee: "2".into(),
            out_fee: "3".into(),
            fee_gamma: "4".into(),
            allowed_extra_profit: "5".into(),
            adjustment_step: "6".into(),
            ma_half_time: "7".into(),
        })
    );
    assert_eq!(
        decode(log_of(
            vec![topic0("NewParameters(uint256,uint256,uint256,uint256,uint256,uint256,uint256)")],
            &[1, 2, 3, 4, 5, 6, 7]
        )),
        pb::log::Log::from(pb::CryptoSwapNewParameters {
            admin_fee: "1".into(),
            mid_fee: "2".into(),
            out_fee: "3".into(),
            fee_gamma: "4".into(),
            allowed_extra_profit: "5".into(),
            adjustment_step: "6".into(),
            ma_half_time: "7".into(),
        })
    );
    assert_eq!(
        decode(log_of(
            vec![topic0("RampAgamma(uint256,uint256,uint256,uint256,uint256,uint256)")],
            &[1, 2, 3, 4, 5, 6]
        )),
        pb::log::Log::from(pb::CryptoSwapRampAgamma {
            initial_a: "1".into(),
            future_a: "2".into(),
            initial_gamma: "3".into(),
            future_gamma: "4".into(),
            initial_time: "5".into(),
            future_time: "6".into(),
        })
    );
    assert_eq!(
        decode(log_of(vec![topic0("StopRampA(uint256,uint256,uint256)")], &[1, 2, 3])),
        pb::log::Log::from(pb::CryptoSwapStopRampA {
            current_a: "1".into(),
            current_gamma: "2".into(),
            time: "3".into(),
        })
    );
}

#[test]
fn cryptoswap_factory_crypto_pool_deployed() {
    let mut data = vec![word(&addr(0xe1)), word(&addr(0xe2)), word(&addr(0xe3))];
    data.extend((1..=10).map(uint));
    data.push(word(&addr(0xe4)));
    let log = log(
        &addr(0x0f),
        vec![topic0(
            "CryptoPoolDeployed(address,address[2],uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,address)",
        )],
        data.concat(),
    );
    assert_eq!(
        deployment(log),
        pb::log::Log::from(pb::CryptoPoolDeployed {
            address: addr(0xc0),
            token: addr(0xe1),
            coins: vec![addr(0xe2), addr(0xe3)],
            a: "1".into(),
            gamma: "2".into(),
            mid_fee: "3".into(),
            out_fee: "4".into(),
            allowed_extra_profit: "5".into(),
            fee_gamma: "6".into(),
            adjustment_step: "7".into(),
            admin_fee: "8".into(),
            ma_half_time: "9".into(),
            initial_price: "10".into(),
            deployer: addr(0xe4),
        })
    );
}

#[test]
fn cryptoswap_factory_admin_events() {
    let log_of = |signature: &str, words: Vec<Vec<u8>>| log(&addr(0x0f), vec![topic0(signature)], words.concat());
    assert_eq!(
        decode(log_of(
            "LiquidityGaugeDeployed(address,address,address)",
            vec![word(&addr(0xf1)), word(&addr(0xf2)), word(&addr(0xf3))]
        )),
        pb::log::Log::from(pb::CryptoSwapFactoryLiquidityGaugeDeployed {
            pool: addr(0xf1),
            token: addr(0xf2),
            gauge: addr(0xf3),
        })
    );
    let pair = || vec![word(&addr(0xf4)), word(&addr(0xf5))];
    assert_eq!(
        decode(log_of("TransferOwnership(address,address)", pair())),
        pb::log::Log::from(pb::CryptoSwapFactoryTransferOwnership {
            old_owner: addr(0xf4),
            new_owner: addr(0xf5),
        })
    );
    assert_eq!(
        decode(log_of("UpdateFeeReceiver(address,address)", pair())),
        pb::log::Log::from(pb::CryptoSwapFactoryUpdateFeeReceiver {
            old_fee_receiver: addr(0xf4),
            new_fee_receiver: addr(0xf5),
        })
    );
    assert_eq!(
        decode(log_of("UpdateGaugeImplementation(address,address)", pair())),
        pb::log::Log::from(pb::CryptoSwapFactoryUpdateGaugeImplementation {
            old_gauge_implementation: addr(0xf4),
            new_gauge_implementation: addr(0xf5),
        })
    );
    assert_eq!(
        decode(log_of("UpdatePoolImplementation(address,address)", pair())),
        pb::log::Log::from(pb::CryptoSwapFactoryUpdatePoolImplementation {
            old_pool_implementation: addr(0xf4),
            new_pool_implementation: addr(0xf5),
        })
    );
    assert_eq!(
        decode(log_of("UpdateTokenImplementation(address,address)", pair())),
        pb::log::Log::from(pb::CryptoSwapFactoryUpdateTokenImplementation {
            old_token_implementation: addr(0xf4),
            new_token_implementation: addr(0xf5),
        })
    );
}

#[test]
fn init_precedes_the_logs_of_its_deployment() {
    // v0.8.0 decodes any contract creation's input tail as a StableSwap
    // constructor: the synthetic Init comes first, then the decoded logs.
    let mut deployment = trx(&[], vec![token_exchange_log()]);
    deployment.calls[0].call_type = CallType::Create.into();
    deployment.calls[0].address = addr(0xc0);
    deployment.input = [vec![0x60; 64], sample_stableswap_constructor().encode()].concat();

    let events = project(&block(vec![deployment])).expect("project");

    let logs = &events.transactions[0].logs;
    assert_eq!(logs.len(), 2);
    let Some(pb::log::Log::Init(init)) = &logs[0].log else { panic!("Init first") };
    assert_eq!(init.address, addr(0xc0));
    assert_eq!((logs[0].ordinal, logs[0].block_index), (1, 0));
    assert!(logs[0].topics.is_empty() && logs[0].data.is_empty());
    assert!(matches!(logs[1].log, Some(pb::log::Log::TokenExchange(_))));
}
