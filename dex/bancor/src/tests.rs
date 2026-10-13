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

/// A log whose parameters are all indexed addresses.
fn addresses_log(signature: &str, addresses: &[u8]) -> Log {
    let mut topics = vec![topic0(signature)];
    topics.extend(addresses.iter().map(|byte| word(&addr(*byte))));
    log(&addr(0x11), topics, vec![])
}

fn conversion_log() -> Log {
    log(
        &addr(0x11),
        vec![
            topic0("Conversion(address,address,address,uint256,uint256,int256)"),
            word(&addr(0xa1)),
            word(&addr(0xa2)),
            word(&addr(0xa3)),
        ],
        [uint(1_000), uint(990), int(-10)].concat(),
    )
}

common::transaction_tests!(conversion_log());

#[test]
fn activation() {
    let log = log(
        &addr(0x11),
        vec![topic0("Activation(uint16,address,bool)"), uint(3), word(&addr(0xa1)), boolean(true)],
        vec![],
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::Activation {
            converter_type: 3,
            anchor: addr(0xa1),
            activated: true,
        })
    );
}

#[test]
fn conversion() {
    assert_eq!(
        decode(conversion_log()),
        pb::log::Log::from(pb::Conversion {
            source_token: addr(0xa1),
            target_token: addr(0xa2),
            trader: addr(0xa3),
            source_amount: "1000".into(),
            target_amount: "990".into(),
            conversion_fee: "-10".into(),
        })
    );
}

#[test]
fn conversion_fee_update() {
    let log = log(
        &addr(0x11),
        vec![topic0("ConversionFeeUpdate(uint32,uint32)")],
        [uint(1_000), uint(2_000)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::ConversionFeeUpdate {
            prev_fee: 1_000,
            new_fee: 2_000
        })
    );
}

#[test]
fn a_wide_conversion_fee_is_truncated_as_in_v0_8_0() {
    // `bigint_to_u64(..).unwrap_or_default() as u32`: 2^32 + 5 keeps its low
    // 32 bits, and a value above u64::MAX becomes 0.
    let log = log(
        &addr(0x11),
        vec![topic0("ConversionFeeUpdate(uint32,uint32)")],
        [uint((1 << 32) + 5), uint(1 << 64)].concat(),
    );
    assert_eq!(decode(log), pb::log::Log::from(pb::ConversionFeeUpdate { prev_fee: 5, new_fee: 0 }));
}

#[test]
fn liquidity_added() {
    let log = log(
        &addr(0x11),
        vec![
            topic0("LiquidityAdded(address,address,uint256,uint256,uint256)"),
            word(&addr(0xa1)),
            word(&addr(0xa2)),
        ],
        [uint(1), uint(2), uint(3)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::LiquidityAdded {
            provider: addr(0xa1),
            reserve_token: addr(0xa2),
            amount: "1".into(),
            new_balance: "2".into(),
            new_supply: "3".into(),
        })
    );
}

#[test]
fn liquidity_removed() {
    let log = log(
        &addr(0x11),
        vec![
            topic0("LiquidityRemoved(address,address,uint256,uint256,uint256)"),
            word(&addr(0xa1)),
            word(&addr(0xa2)),
        ],
        [uint(4), uint(5), uint(6)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::LiquidityRemoved {
            provider: addr(0xa1),
            reserve_token: addr(0xa2),
            amount: "4".into(),
            new_balance: "5".into(),
            new_supply: "6".into(),
        })
    );
}

#[test]
fn owner_update() {
    assert_eq!(
        decode(addresses_log("OwnerUpdate(address,address)", &[0xa1, 0xa2])),
        pb::log::Log::from(pb::OwnerUpdate {
            prev_owner: addr(0xa1),
            new_owner: addr(0xa2),
        })
    );
}

#[test]
fn token_rate_update() {
    let log = log(
        &addr(0x11),
        vec![topic0("TokenRateUpdate(address,address,uint256,uint256)"), word(&addr(0xa1)), word(&addr(0xa2))],
        [uint(7), uint(8)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::TokenRateUpdate {
            token1: addr(0xa1),
            token2: addr(0xa2),
            rate_n: "7".into(),
            rate_d: "8".into(),
        })
    );
}

#[test]
fn registry_events() {
    assert_eq!(
        decode(addresses_log("ConverterAnchorAdded(address)", &[0xb1])),
        pb::log::Log::from(pb::ConverterAnchorAdded { anchor: addr(0xb1) })
    );
    assert_eq!(
        decode(addresses_log("ConverterAnchorRemoved(address)", &[0xb2])),
        pb::log::Log::from(pb::ConverterAnchorRemoved { anchor: addr(0xb2) })
    );
    assert_eq!(
        decode(addresses_log("ConvertibleTokenAdded(address,address)", &[0xb3, 0xb4])),
        pb::log::Log::from(pb::ConvertibleTokenAdded {
            convertible_token: addr(0xb3),
            smart_token: addr(0xb4),
        })
    );
    assert_eq!(
        decode(addresses_log("ConvertibleTokenRemoved(address,address)", &[0xb5, 0xb6])),
        pb::log::Log::from(pb::ConvertibleTokenRemoved {
            convertible_token: addr(0xb5),
            smart_token: addr(0xb6),
        })
    );
    assert_eq!(
        decode(addresses_log("LiquidityPoolAdded(address)", &[0xb7])),
        pb::log::Log::from(pb::LiquidityPoolAdded { liquidity_pool: addr(0xb7) })
    );
    assert_eq!(
        decode(addresses_log("LiquidityPoolRemoved(address)", &[0xb8])),
        pb::log::Log::from(pb::LiquidityPoolRemoved { liquidity_pool: addr(0xb8) })
    );
    assert_eq!(
        decode(addresses_log("SmartTokenAdded(address)", &[0xb9])),
        pb::log::Log::from(pb::SmartTokenAdded { smart_token: addr(0xb9) })
    );
    assert_eq!(
        decode(addresses_log("SmartTokenRemoved(address)", &[0xba])),
        pb::log::Log::from(pb::SmartTokenRemoved { smart_token: addr(0xba) })
    );
}

#[test]
fn new_converter() {
    let log = log(
        &addr(0x12),
        vec![topic0("NewConverter(uint16,address,address)"), uint(3), word(&addr(0xc1)), word(&addr(0xc2))],
        vec![],
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::NewConverter {
            converter_type: 3,
            converter: addr(0xc1),
            owner: addr(0xc2),
        })
    );
}

#[test]
fn legacy_new_converter_is_type_1() {
    assert_eq!(
        decode(addresses_log("NewConverter(address,address)", &[0xc3, 0xc4])),
        pb::log::Log::from(pb::NewConverter {
            converter_type: 1,
            converter: addr(0xc3),
            owner: addr(0xc4),
        })
    );
}

#[test]
fn features_addition_and_removal() {
    let log_with = |signature: &str, features: u128| log(&addr(0x13), vec![topic0(signature), word(&addr(0xd1))], uint(features));
    assert_eq!(
        decode(log_with("FeaturesAddition(address,uint256)", 1)),
        pb::log::Log::from(pb::FeaturesAddition {
            address: addr(0xd1),
            features: "1".into(),
        })
    );
    assert_eq!(
        decode(log_with("FeaturesRemoval(address,uint256)", 2)),
        pb::log::Log::from(pb::FeaturesRemoval {
            address: addr(0xd1),
            features: "2".into(),
        })
    );
}
