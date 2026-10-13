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

const LAUNCHPAD: u8 = 0x5f;

fn token_purchased_log() -> Log {
    log(
        &addr(LAUNCHPAD),
        vec![
            topic0("TokenPurchased(address,address,uint256,uint256,uint256,uint256)"),
            word(&addr(0xa1)),
            word(&addr(0xa2)),
        ],
        [uint(1_000), uint(10), uint(5_000), uint(90_000)].concat(),
    )
}

fn legacy_token_create_log(nft_max_supply: Vec<u8>, nft_threshold: Vec<u8>) -> Log {
    log(
        &addr(LAUNCHPAD),
        vec![topic0("TokenCreate(address,address,uint64,uint64,string,string)")],
        encode(&[
            Abi::Word(word(&addr(0xa1))),
            Abi::Word(word(&addr(0xa2))),
            Abi::Word(nft_max_supply),
            Abi::Word(nft_threshold),
            Abi::Bytes(b"Sun Token".to_vec()),
            Abi::Bytes(b"SUN".to_vec()),
        ]),
    )
}

/// A log whose parameters are two indexed addresses.
fn change_log(signature: &str) -> Log {
    log(&addr(LAUNCHPAD), vec![topic0(signature), word(&addr(0xb1)), word(&addr(0xb2))], vec![])
}

/// A log whose parameters are two fee values.
fn fee_log(signature: &str) -> Log {
    log(&addr(LAUNCHPAD), vec![topic0(signature)], [uint(100), uint(200)].concat())
}

common::transaction_tests!(token_purchased_log());

#[test]
fn token_create() {
    let log = log(
        &addr(LAUNCHPAD),
        vec![topic0("TokenCreate(address,uint256,address)")],
        [word(&addr(0xa1)), uint(17), word(&addr(0xa2))].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::TokenCreate {
            token_address: addr(0xa1),
            token_index: "17".into(),
            creator: addr(0xa2),
        })
    );
}

#[test]
fn legacy_token_create() {
    assert_eq!(
        decode(legacy_token_create_log(uint(10_000), uint(100))),
        pb::log::Log::from(pb::TokenCreateLegacy {
            token_address: addr(0xa1),
            creator: addr(0xa2),
            nft_threshold: 100,
            nft_max_supply: 10_000,
            name: "Sun Token".into(),
            symbol: "SUN".into(),
        })
    );
}

#[test]
fn legacy_token_create_fails_above_u64_max() {
    // v0.8.0's `to_u64()` panicked; the port fails the block instead.
    let log = legacy_token_create_log(uint(1 << 64), uint(1));
    let error = project(&block(vec![trx(&addr(0x01), vec![log])])).unwrap_err();
    assert_eq!(
        error.to_string(),
        "TokenCreate.nftMaxSupply: BigInt '18446744073709551616' is too large to fit into u64"
    );
}

#[test]
fn launch_pending() {
    let log = log(&addr(LAUNCHPAD), vec![topic0("LaunchPending(address)")], word(&addr(0xa1)));
    assert_eq!(decode(log), pb::log::Log::from(pb::LaunchPending { token: addr(0xa1) }));
}

#[test]
fn role_changes() {
    assert_eq!(
        decode(change_log("LauncherChanged(address,address)")),
        pb::log::Log::from(pb::LauncherChanged {
            old_launcher: addr(0xb1),
            new_launcher: addr(0xb2),
        })
    );
    assert_eq!(
        decode(change_log("OperatorChanged(address,address)")),
        pb::log::Log::from(pb::OperatorChanged {
            old_operator: addr(0xb1),
            new_operator: addr(0xb2),
        })
    );
    assert_eq!(
        decode(change_log("OwnerChanged(address,address)")),
        pb::log::Log::from(pb::OwnerChanged {
            old_owner: addr(0xb1),
            new_owner: addr(0xb2),
        })
    );
    assert_eq!(
        decode(change_log("PendingOwnerSet(address,address)")),
        pb::log::Log::from(pb::PendingOwnerSet {
            old_pending_owner: addr(0xb1),
            new_pending_owner: addr(0xb2),
        })
    );
}

#[test]
fn fee_settings() {
    let (old_fee, new_fee) = (String::from("100"), String::from("200"));
    assert_eq!(
        decode(fee_log("MinTxFeeSet(uint256,uint256)")),
        pb::log::Log::from(pb::MinTxFeeSet {
            old_fee: old_fee.clone(),
            new_fee: new_fee.clone(),
        })
    );
    assert_eq!(
        decode(fee_log("MintFeeSet(uint256,uint256)")),
        pb::log::Log::from(pb::MintFeeSet {
            old_fee: old_fee.clone(),
            new_fee: new_fee.clone(),
        })
    );
    assert_eq!(
        decode(fee_log("PurchaseFeeSet(uint256,uint256)")),
        pb::log::Log::from(pb::PurchaseFeeSet {
            old_fee: old_fee.clone(),
            new_fee: new_fee.clone(),
        })
    );
    assert_eq!(
        decode(fee_log("SaleFeeSet(uint256,uint256)")),
        pb::log::Log::from(pb::SaleFeeSet { old_fee, new_fee })
    );
}

#[test]
fn token_launched() {
    let log = log(&addr(LAUNCHPAD), vec![topic0("TokenLaunched(address)"), word(&addr(0xa1))], vec![]);
    assert_eq!(decode(log), pb::log::Log::from(pb::TokenLaunched { token: addr(0xa1) }));
}

#[test]
fn token_purchased() {
    assert_eq!(
        decode(token_purchased_log()),
        pb::log::Log::from(pb::TokenPurchased {
            token: addr(0xa1),
            buyer: addr(0xa2),
            trx_amount: "1000".into(),
            fee: "10".into(),
            token_amount: "5000".into(),
            token_reserve: "90000".into(),
        })
    );
}

#[test]
fn token_sold() {
    let log = log(
        &addr(LAUNCHPAD),
        vec![
            topic0("TokenSold(address,address,uint256,uint256,uint256)"),
            word(&addr(0xa1)),
            word(&addr(0xa3)),
        ],
        [uint(2_000), uint(20), uint(6_000)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::TokenSold {
            token: addr(0xa1),
            seller: addr(0xa3),
            trx_amount: "2000".into(),
            fee: "20".into(),
            token_amount: "6000".into(),
        })
    );
}
