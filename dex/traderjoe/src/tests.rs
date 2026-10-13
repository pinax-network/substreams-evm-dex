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

/// A `bytes32` holding `high` in its upper and `low` in its lower 128 bits.
fn packed(high: u128, low: u128) -> Vec<u8> {
    [high.to_be_bytes(), low.to_be_bytes()].concat()
}

fn swap_log(id: Vec<u8>) -> Log {
    log(
        &addr(0x11),
        vec![
            topic0("Swap(address,address,uint24,bytes32,bytes32,uint24,bytes32,bytes32)"),
            word(&addr(0xa1)),
            word(&addr(0xa2)),
        ],
        [id, packed(0, 1_000), packed(2_000, 0), uint(7), packed(0, 3), packed(0, 1)].concat(),
    )
}

fn bins_log(signature: &str) -> Log {
    log(
        &addr(0x11),
        vec![topic0(signature), word(&addr(0xa1)), word(&addr(0xa2))],
        encode(&[
            Abi::Array(vec![Abi::Word(uint(8_388_608)), Abi::Word(uint(8_388_609))]),
            Abi::Array(vec![Abi::Word(packed(5, 6)), Abi::Word(packed(7, 8))]),
        ]),
    )
}

common::transaction_tests!(swap_log(uint(8_388_608)));

#[test]
fn swap_reads_the_high_half_as_x_as_in_v0_8_0() {
    assert_eq!(
        decode(swap_log(uint(8_388_608))),
        pb::log::Log::from(pb::Swap {
            sender: addr(0xa1),
            to: addr(0xa2),
            id: 8_388_608,
            amount_in_x: "0".into(),
            amount_in_y: "1000".into(),
            amount_out_x: "2000".into(),
            amount_out_y: "0".into(),
            volatility_accumulator: 7,
            total_fees_x: "0".into(),
            total_fees_y: "3".into(),
            protocol_fees_x: "0".into(),
            protocol_fees_y: "1".into(),
        })
    );
}

#[test]
fn a_bin_id_above_u64_max_fails_the_block() {
    // v0.8.0's `to_u64()` panicked; the port fails the block instead.
    let error = project(&block(vec![trx(&addr(0x01), vec![swap_log(uint(1 << 64))])])).unwrap_err();
    assert_eq!(error.to_string(), "Swap.id: BigInt '18446744073709551616' is too large to fit into u64");
}

#[test]
fn deposited_to_bins() {
    assert_eq!(
        decode(bins_log("DepositedToBins(address,address,uint256[],bytes32[])")),
        pb::log::Log::from(pb::DepositedToBins {
            sender: addr(0xa1),
            to: addr(0xa2),
            ids: vec![8_388_608, 8_388_609],
            amounts: vec![packed(5, 6), packed(7, 8)],
        })
    );
}

#[test]
fn withdrawn_from_bins() {
    assert_eq!(
        decode(bins_log("WithdrawnFromBins(address,address,uint256[],bytes32[])")),
        pb::log::Log::from(pb::WithdrawnFromBins {
            sender: addr(0xa1),
            to: addr(0xa2),
            ids: vec![8_388_608, 8_388_609],
            amounts: vec![packed(5, 6), packed(7, 8)],
        })
    );
}

#[test]
fn composition_fees() {
    let log = log(
        &addr(0x11),
        vec![topic0("CompositionFees(address,uint24,bytes32,bytes32)"), word(&addr(0xa1))],
        [uint(8_388_610), packed(9, 10), packed(11, 12)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::CompositionFees {
            sender: addr(0xa1),
            id: 8_388_610,
            total_fees_x: "9".into(),
            total_fees_y: "10".into(),
            protocol_fees_x: "11".into(),
            protocol_fees_y: "12".into(),
        })
    );
}

#[test]
fn lb_pair_created() {
    let log = log(
        &addr(0x12),
        vec![
            topic0("LBPairCreated(address,address,uint256,address,uint256)"),
            word(&addr(0xb1)),
            word(&addr(0xb2)),
            uint(25),
        ],
        [word(&addr(0xb3)), uint(4)].concat(),
    );
    assert_eq!(
        decode(log),
        pb::log::Log::from(pb::LbPairCreated {
            token_x: addr(0xb1),
            token_y: addr(0xb2),
            bin_step: 25,
            lb_pair: addr(0xb3),
            pid: 4,
        })
    );
}
