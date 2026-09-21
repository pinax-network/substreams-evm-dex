use proto::pb::{
    dex::pool_state::v1::BlockPoolState,
    uniswap::{v2 as pb_v2, v3 as pb_v3},
};
use substreams::errors::Error;

mod v2;
mod v3;

use substreams_ethereum::pb::eth::v2::{block::DetailLevel, Block, TransactionTraceStatus};

/// Project complete Extended blocks without RPC, price policy or pool admission.
pub fn project(block: &Block) -> Result<BlockPoolState, Error> {
    combine(v2::extract(block)?, v3::extract(block)?)
}

fn validate_block(block: &Block) -> Result<(), Error> {
    if block.detail_level != DetailLevel::DetaillevelExtended as i32 {
        return Err(Error::msg("Extended blocks required"));
    }
    for transaction in &block.transaction_traces {
        if !matches!(
            TransactionTraceStatus::try_from(transaction.status),
            Ok(TransactionTraceStatus::Succeeded | TransactionTraceStatus::Failed | TransactionTraceStatus::Reverted)
        ) {
            return Err(Error::msg("invalid transaction status"));
        }
        if transaction.status == TransactionTraceStatus::Succeeded as i32 && transaction.calls.is_empty() {
            return Err(Error::msg("successful transaction is missing Extended calls"));
        }
    }
    Ok(())
}

// Only the WASM build exports the package's single canonical map_events.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;
    #[substreams::handlers::map]
    fn map_events(block: Block) -> Result<BlockPoolState, Error> {
        project(&block)
    }
}

fn combine(v2: pb_v2::BlockPoolCloses, v3: pb_v3::BlockPoolChanges) -> Result<BlockPoolState, Error> {
    if v2.block_hash.len() != 32
        || v2.parent_hash.len() != 32
        || v2.timestamp_seconds < 0
        || !(0..1_000_000_000).contains(&v2.timestamp_nanos)
        || v2.block_number != v3.block_number
        || v2.block_hash != v3.block_hash
        || v2.parent_hash != v3.parent_hash
        || v2.timestamp_seconds != v3.timestamp_seconds
        || v2.timestamp_nanos != v3.timestamp_nanos
    {
        return Err(Error::msg("protocol outputs do not describe the same complete block"));
    }
    Ok(BlockPoolState {
        block_number: v2.block_number,
        block_hash: v2.block_hash,
        parent_hash: v2.parent_hash,
        timestamp_seconds: v2.timestamp_seconds,
        timestamp_nanos: v2.timestamp_nanos,
        v2_pools: v2.pools,
        v3_pools: v3.pools,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use substreams_ethereum::pb::eth::v2::{BlockHeader, Call, TransactionReceipt, TransactionTrace};

    fn extended_block() -> Block {
        Block {
            detail_level: DetailLevel::DetaillevelExtended as i32,
            number: 100,
            hash: vec![1; 32],
            header: Some(BlockHeader {
                number: 100,
                parent_hash: vec![2; 32],
                timestamp: Some(Default::default()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn empty_extended_blocks_and_successful_empty_calls_emit_complete_headers() {
        let mut input = extended_block();
        let timestamp = input.header.as_mut().unwrap().timestamp.as_mut().unwrap();
        timestamp.seconds = 1_700_000_000;
        timestamp.nanos = 123_456_789;
        for with_transaction in [false, true] {
            if with_transaction {
                // Complete call data is sufficient; receipts are not consulted.
                input.transaction_traces.push(TransactionTrace {
                    status: TransactionTraceStatus::Succeeded as i32,
                    calls: vec![Call::default()],
                    ..Default::default()
                });
            }
            let out = project(&input).unwrap();
            assert_eq!(out.block_number, 100);
            assert_eq!(out.block_hash, vec![1; 32]);
            assert_eq!(out.parent_hash, vec![2; 32]);
            assert_eq!((out.timestamp_seconds, out.timestamp_nanos), (1_700_000_000, 123_456_789));
            assert!(out.v2_pools.is_empty() && out.v3_pools.is_empty());
        }
    }

    #[test]
    fn base_and_unknown_detail_are_rejected_even_without_transactions() {
        for detail_level in [DetailLevel::DetaillevelBase as i32, -1, 99] {
            let mut input = extended_block();
            input.detail_level = detail_level;
            assert!(project(&input).is_err());
        }
    }

    #[test]
    fn successful_transactions_require_calls_even_with_a_receipt() {
        for receipt in [None, Some(TransactionReceipt::default())] {
            let mut input = extended_block();
            input.transaction_traces.push(TransactionTrace {
                status: TransactionTraceStatus::Succeeded as i32,
                receipt,
                ..Default::default()
            });
            assert!(project(&input).is_err());
        }
    }

    #[test]
    fn unknown_transaction_status_never_becomes_an_empty_complete_observation() {
        for status in [-1, 0, 4] {
            let mut input = extended_block();
            input.transaction_traces.push(TransactionTrace {
                status,
                calls: vec![Call::default()],
                ..Default::default()
            });
            assert!(project(&input).is_err());
        }
    }

    #[test]
    fn invalid_source_headers_and_timestamp_bounds_fail_closed() {
        for mutation in 0..9 {
            let mut input = extended_block();
            match mutation {
                0 => input.header = None,
                1 => input.hash.pop().map(|_| ()).unwrap(),
                2 => input.header.as_mut().unwrap().number += 1,
                3 => input.header.as_mut().unwrap().parent_hash.clear(),
                4 => input.header.as_mut().unwrap().timestamp = None,
                5 => input.header.as_mut().unwrap().timestamp.as_mut().unwrap().seconds = -1,
                6 => input.header.as_mut().unwrap().timestamp.as_mut().unwrap().nanos = -1,
                7 => input.header.as_mut().unwrap().timestamp.as_mut().unwrap().nanos = 1_000_000_000,
                _ => input.hash.push(0),
            }
            assert!(project(&input).is_err(), "accepted mutation {mutation}");
        }
    }

    fn fixture() -> (pb_v2::BlockPoolCloses, pb_v3::BlockPoolChanges) {
        let v2 = pb_v2::BlockPoolCloses {
            block_number: 100,
            block_hash: vec![1; 32],
            parent_hash: vec![2; 32],
            timestamp_seconds: 123,
            timestamp_nanos: 456,
            pools: vec![pb_v2::PoolClose {
                pool: vec![3; 20],
                reserve0: "5192296858534827628530496329220095".into(),
                reserve1: "0".into(),
                block_log_index: 1,
                ordinal: 10,
                invalid: false,
            }],
        };
        let v3 = pb_v3::BlockPoolChanges {
            block_number: v2.block_number,
            block_hash: v2.block_hash.clone(),
            parent_hash: v2.parent_hash.clone(),
            timestamp_seconds: v2.timestamp_seconds,
            timestamp_nanos: v2.timestamp_nanos,
            pools: vec![pb_v3::PoolChanges {
                pool: vec![4; 20],
                changes: vec![
                    pb_v3::PoolChange {
                        block_log_index: 2,
                        ordinal: 20,
                        change: Some(pb_v3::pool_change::Change::Swap(pb_v3::PoolPriceState {
                            sqrt_price_x96: "79228162514264337593543950336".into(),
                            tick: -1,
                            liquidity: "10".into(),
                        })),
                    },
                    pb_v3::PoolChange {
                        block_log_index: 3,
                        ordinal: 30,
                        change: Some(pb_v3::pool_change::Change::Liquidity(pb_v3::PoolLiquidityChange {
                            tick_lower: -100,
                            tick_upper: 100,
                            liquidity_delta: "-7".into(),
                        })),
                    },
                    pb_v3::PoolChange {
                        block_log_index: 4,
                        ordinal: 40,
                        change: Some(pb_v3::pool_change::Change::Invalid(pb_v3::InvalidPoolChange { event_name: "Swap".into() })),
                    },
                ],
            }],
        };
        (v2, v3)
    }
    #[test]
    fn both_protocols_round_trip_without_losing_precision_order_or_invalid_markers() {
        let (v2, v3) = fixture();
        let output = combine(v2.clone(), v3.clone()).unwrap();
        let output = BlockPoolState::decode(output.encode_to_vec().as_slice()).unwrap();
        assert_eq!(output.v2_pools, v2.pools);
        assert_eq!(output.v3_pools, v3.pools);
        assert_eq!(output.timestamp_nanos, 456);
    }
    #[test]
    fn empty_protocol_outputs_still_emit_the_complete_block() {
        for empty_v2 in [true, false] {
            for empty_v3 in [true, false] {
                let (mut v2, mut v3) = fixture();
                if empty_v2 {
                    v2.pools.clear();
                }
                if empty_v3 {
                    v3.pools.clear();
                }
                let output = combine(v2, v3).unwrap();
                assert_eq!(output.block_number, 100);
                assert_eq!(output.v2_pools.is_empty(), empty_v2);
                assert_eq!(output.v3_pools.is_empty(), empty_v3);
            }
        }
    }
    #[test]
    fn every_header_mismatch_and_invalid_envelope_is_rejected() {
        for mutation in 0..9 {
            let (mut v2, mut v3) = fixture();
            match mutation {
                0 => v3.block_number += 1,
                1 => v3.block_hash[0] ^= 1,
                2 => v3.parent_hash[0] ^= 1,
                3 => v3.timestamp_seconds += 1,
                4 => v3.timestamp_nanos += 1,
                5 => {
                    v2.block_hash.clear();
                    v3.block_hash.clear();
                }
                6 => {
                    v2.parent_hash.clear();
                    v3.parent_hash.clear();
                }
                7 => {
                    v2.timestamp_seconds = -1;
                    v3.timestamp_seconds = -1;
                }
                _ => {
                    v2.timestamp_nanos = 1_000_000_000;
                    v3.timestamp_nanos = 1_000_000_000;
                }
            }
            assert!(combine(v2, v3).is_err(), "accepted mutation {mutation}");
        }
    }
}
