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

/// All three parameters of the V1 exchange events are indexed.
fn exchange_log(signature: &str, who: u8, first: u128, second: u128) -> Log {
    log(&addr(0x11), vec![topic0(signature), word(&addr(who)), uint(first), uint(second)], vec![])
}

common::transaction_tests!(exchange_log("TokenPurchase(address,uint256,uint256)", 0xa1, 1, 2));

#[test]
fn token_purchase() {
    assert_eq!(
        decode(exchange_log("TokenPurchase(address,uint256,uint256)", 0xa1, 1_000, 2_000)),
        pb::log::Log::from(pb::TokenPurchase {
            buyer: addr(0xa1),
            eth_sold: "1000".into(),
            tokens_bought: "2000".into(),
        })
    );
}

#[test]
fn eth_purchase() {
    assert_eq!(
        decode(exchange_log("EthPurchase(address,uint256,uint256)", 0xa2, 3_000, 4_000)),
        pb::log::Log::from(pb::EthPurchase {
            buyer: addr(0xa2),
            tokens_sold: "3000".into(),
            eth_bought: "4000".into(),
        })
    );
}

#[test]
fn justswap_trx_purchase_is_an_eth_purchase() {
    assert_eq!(
        decode(exchange_log("TrxPurchase(address,uint256,uint256)", 0xa3, 5, 6)),
        pb::log::Log::from(pb::EthPurchase {
            buyer: addr(0xa3),
            tokens_sold: "5".into(),
            eth_bought: "6".into(),
        })
    );
}

#[test]
fn add_liquidity() {
    assert_eq!(
        decode(exchange_log("AddLiquidity(address,uint256,uint256)", 0xa4, 7, u128::MAX)),
        pb::log::Log::from(pb::AddLiquidity {
            provider: addr(0xa4),
            eth_amount: "7".into(),
            token_amount: u128::MAX.to_string(),
        })
    );
}

#[test]
fn remove_liquidity() {
    assert_eq!(
        decode(exchange_log("RemoveLiquidity(address,uint256,uint256)", 0xa5, 9, 10)),
        pb::log::Log::from(pb::RemoveLiquidity {
            provider: addr(0xa5),
            eth_amount: "9".into(),
            token_amount: "10".into(),
        })
    );
}

#[test]
fn new_exchange() {
    let log = log(
        &addr(0x12),
        vec![topic0("NewExchange(address,address)"), word(&addr(0xb1)), word(&addr(0xb2))],
        vec![],
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::NewExchange {
            token: addr(0xb1),
            exchange: addr(0xb2),
        })
    );
}

#[test]
fn a_full_word_amount_is_kept_exactly() {
    let log = log(
        &addr(0x11),
        vec![topic0("TokenPurchase(address,uint256,uint256)"), word(&addr(0xa1)), vec![0xff; 32], uint(0)],
        vec![],
    );
    let pb::log::Log::TokenPurchase(event) = decode(log) else {
        panic!("TokenPurchase")
    };
    assert_eq!(event.eth_sold, "115792089237316195423570985008687907853269984665640564039457584007913129639935");
}
