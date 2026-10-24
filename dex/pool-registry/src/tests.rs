use super::*;
use dex_pools::eth::{Block, Call, CallType, Log, TransactionReceipt, TransactionTrace, TransactionTraceStatus};
use std::collections::HashMap;
use tiny_keccak::{Hasher, Keccak};

fn at(number: u64) -> BlockRef {
    BlockRef {
        number,
        hash: [number as u8; 32],
    }
}
fn entry(key: u8, tokens: &[u8]) -> PoolCreated {
    PoolCreated {
        address: hex::encode([key; 20]),
        pool: Pool {
            tokens: tokens.iter().map(|byte| vec![*byte; 20]).collect(),
            factory: vec![0xfa; 20],
        }
        .into(),
    }
}
fn seed(pools: Vec<PoolCreated>) -> Seed {
    Seed {
        chain: "ethereum".into(),
        revision: "synthetic-v1".into(),
        at: at(10),
        completeness: Completeness::Complete,
        pools,
    }
}
fn registry() -> Registry {
    Registry::new(seed(vec![entry(1, &[10, 11])]), Limits::default()).unwrap()
}
fn commit(registry: &mut Registry, number: u64, entries: Vec<PoolCreated>) {
    registry
        .prepare_block(at(number), at(number - 1).hash, &Pools { pools: entries })
        .unwrap()
        .commit();
}

#[test]
fn full_shapes_preserve_order_repeats_zero_ids_tokens_and_empty_factory() {
    let mut zero = entry(0, &[0, 2, 2, 3, 4]);
    zero.pool.as_option_mut().unwrap().factory.clear();
    let mut v4 = entry(4, &[0, 1]);
    v4.address = hex::encode([4; 32]);
    let entries = vec![zero.clone(), entry(1, &[]), entry(2, &[1]), entry(3, &[1, 2, 3]), v4.clone()];
    let registry = Registry::new(seed(entries.clone()), Limits::default()).unwrap();
    assert_eq!(registry.len(), entries.len());
    for value in entries {
        assert_eq!(registry.get_pool(&hex::decode(value.address).unwrap()).as_ref(), value.pool.as_option());
    }
    assert!(registry.get_pool(&[4; 20]).is_none());
    assert_eq!(registry.get_pool(&[4; 32]), v4.pool.into_option());
}

#[test]
fn first_writer_wins_in_seed_across_blocks_and_within_block() {
    let mut registry = Registry::new(seed(vec![entry(1, &[1]), entry(1, &[9])]), Limits::default()).unwrap();
    let update = Pools {
        pools: vec![entry(1, &[8]), entry(2, &[2, 3]), entry(2, &[8, 9])],
    };
    let prepared = registry.prepare_block(at(11), at(10).hash, &update).unwrap();
    assert_eq!(prepared.get_pool(&[1; 20]).unwrap().tokens, vec![vec![1; 20]]);
    assert_eq!(prepared.get_pool(&[2; 20]).unwrap().tokens, vec![vec![2; 20], vec![3; 20]]);
    assert_eq!(prepared.new_pools().count(), 1);
    prepared.commit();
    commit(&mut registry, 12, vec![entry(2, &[9]), entry(3, &[])]);
    registry.undo_to(&at(11)).unwrap();
    assert!(registry.pool(&[3; 20]).is_none());
    assert_eq!(registry.get_pool(&[2; 20]).unwrap().tokens, vec![vec![2; 20], vec![3; 20]]);
    registry.undo_to(&at(10)).unwrap();
    assert!(registry.pool(&[2; 20]).is_none());
    assert_eq!(registry.get_pool(&[1; 20]).unwrap().tokens, vec![vec![1; 20]]);
}

#[test]
fn failed_or_dropped_preparation_cannot_leak_any_pool_or_history() {
    let mut registry = registry();
    let good = entry(2, &[2]);
    let mut bad = entry(3, &[3]);
    bad.pool.as_option_mut().unwrap().tokens[0].pop();
    assert!(registry
        .prepare_block(
            at(11),
            at(10).hash,
            &Pools {
                pools: vec![good.clone(), bad]
            }
        )
        .is_err());
    assert_eq!(registry.head(), &at(10));
    assert_eq!(registry.len(), 1);
    {
        let prepared = registry.prepare_block(at(11), at(10).hash, &Pools { pools: vec![good] }).unwrap();
        assert!(prepared.get_pool(&[2; 20]).is_some());
    }
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.head(), &at(10));
    assert!(registry.can_undo(&at(11)).is_err());
}

#[test]
fn malformed_and_noncanonical_shapes_are_explicit_admission_failures() {
    let mut cases = Vec::new();
    for address in [
        "".into(),
        "ab".repeat(19),
        "ab".repeat(21),
        "0x".to_string() + &"ab".repeat(20),
        "AB".repeat(20),
        "gg".repeat(20),
    ] {
        let mut value = entry(2, &[]);
        value.address = address;
        cases.push(value);
    }
    let mut missing = entry(2, &[]);
    missing.pool = Default::default();
    cases.push(missing);
    for width in [0, 19, 21, 32] {
        let mut value = entry(2, &[1]);
        value.pool.as_option_mut().unwrap().tokens[0] = vec![1; width];
        cases.push(value);
    }
    for width in [1, 19, 21, 32] {
        let mut value = entry(2, &[]);
        value.pool.as_option_mut().unwrap().factory = vec![1; width];
        cases.push(value);
    }
    for value in cases {
        assert!(Registry::new(seed(vec![value.clone()]), Limits::default()).is_err());
        let mut registry = registry();
        assert!(registry
            .prepare_block(
                at(11),
                at(10).hash,
                &Pools {
                    pools: vec![entry(4, &[]), value]
                }
            )
            .is_err());
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.head(), &at(10));
    }
}

#[test]
fn even_ignored_duplicate_inputs_are_validated_and_count_against_raw_bounds() {
    let mut registry = registry();
    let mut bad = entry(1, &[]);
    bad.pool.as_option_mut().unwrap().factory.push(1);
    assert!(registry.prepare_block(at(11), at(10).hash, &Pools { pools: vec![bad] }).is_err());
    let limits = Limits {
        max_pools_per_block: 1,
        ..Limits::default()
    };
    let mut registry = Registry::new(seed(vec![entry(1, &[])]), limits).unwrap();
    assert!(registry
        .prepare_block(
            at(11),
            at(10).hash,
            &Pools {
                pools: vec![entry(1, &[]), entry(1, &[])]
            }
        )
        .is_err());
}

#[test]
fn resource_bounds_fail_before_commit() {
    for limits in [
        Limits {
            max_pools: 1,
            ..Limits::default()
        },
        Limits {
            max_tokens_per_pool: 1,
            ..Limits::default()
        },
        Limits {
            max_undo_pools: 1,
            ..Limits::default()
        },
    ] {
        let mut registry = Registry::new(seed(vec![]), limits).unwrap();
        assert!(registry
            .prepare_block(
                at(11),
                at(10).hash,
                &Pools {
                    pools: vec![entry(2, &[1, 2]), entry(3, &[])]
                }
            )
            .is_err());
        assert!(registry.is_empty());
        assert_eq!(registry.head(), &at(10));
    }
    assert!(Registry::new(
        seed(vec![entry(1, &[]), entry(1, &[])]),
        Limits {
            max_pools: 1,
            ..Limits::default()
        }
    )
    .is_err());
    assert!(Registry::new(
        seed(vec![]),
        Limits {
            max_undo_blocks: 0,
            ..Limits::default()
        }
    )
    .is_err());
}

#[test]
fn empty_blocks_still_enforce_height_parent_and_undo_continuity() {
    let mut registry = registry();
    assert!(registry.prepare_block(at(12), at(10).hash, &Pools::default()).is_err());
    assert!(registry.prepare_block(at(11), [99; 32], &Pools::default()).is_err());
    commit(&mut registry, 11, vec![]);
    commit(&mut registry, 12, vec![]);
    assert_eq!(registry.head(), &at(12));
    assert!(registry.undo_to(&BlockRef { number: 11, hash: [99; 32] }).is_err());
    registry.undo_to(&at(10)).unwrap();
    assert_eq!(registry.head(), &at(10));
    let mut overflow = Registry::new(
        Seed {
            at: at(u64::MAX),
            ..seed(vec![])
        },
        Limits::default(),
    )
    .unwrap();
    assert!(overflow.prepare_block(at(0), at(u64::MAX).hash, &Pools::default()).is_err());
}

#[test]
fn undo_fork_can_replace_only_metadata_introduced_on_the_removed_branch() {
    let mut registry = registry();
    commit(&mut registry, 11, vec![entry(2, &[2])]);
    commit(&mut registry, 12, vec![entry(3, &[3])]);
    {
        let _prepared = registry.prepare_undo(&at(10)).unwrap();
    }
    assert_eq!(registry.len(), 3);
    registry.prepare_undo(&at(10)).unwrap().commit();
    let fork = BlockRef { number: 11, hash: [77; 32] };
    registry
        .prepare_block(
            fork,
            at(10).hash,
            &Pools {
                pools: vec![entry(1, &[99]), entry(2, &[22])],
            },
        )
        .unwrap()
        .commit();
    assert_eq!(registry.get_pool(&[1; 20]).unwrap().tokens, vec![vec![10; 20], vec![11; 20]]);
    assert_eq!(registry.get_pool(&[2; 20]).unwrap().tokens, vec![vec![22; 20]]);
    assert!(registry.can_undo(&at(11)).is_err());
}

#[test]
fn undo_eviction_retains_metadata_and_exact_floor_identity() {
    for limits in [
        Limits {
            max_undo_blocks: 2,
            ..Limits::default()
        },
        Limits {
            max_undo_pools: 2,
            ..Limits::default()
        },
    ] {
        let mut registry = Registry::new(seed(vec![]), limits).unwrap();
        commit(&mut registry, 11, vec![entry(1, &[])]);
        commit(&mut registry, 12, vec![entry(2, &[])]);
        commit(&mut registry, 13, vec![entry(3, &[])]);
        assert_eq!(registry.len(), 3);
        assert!(registry.undo_to(&at(10)).is_err());
        assert_eq!(registry.len(), 3);
        registry.undo_to(&at(11)).unwrap();
        assert_eq!(registry.len(), 1);
        assert!(registry.pool(&[1; 20]).is_some());
        assert_eq!(registry.head(), &at(11));
    }
}

#[test]
fn finality_is_exact_canonical_monotonic_and_transactional() {
    let mut registry = registry();
    commit(&mut registry, 11, vec![entry(2, &[])]);
    commit(&mut registry, 12, vec![entry(3, &[])]);
    {
        let _prepared = registry.prepare_finalize(&at(11)).unwrap();
    }
    assert!(registry.finalized().is_none());
    for target in [at(13), BlockRef { number: 11, hash: [99; 32] }] {
        assert!(registry.finalize(&target).is_err());
        assert!(registry.finalized().is_none());
    }
    registry.prepare_finalize(&at(11)).unwrap().commit();
    assert_eq!(registry.finalized(), Some(&at(11)));
    assert!(registry.finalize(&at(10)).is_err());
    assert!(registry.undo_to(&at(10)).is_err());
    registry.undo_to(&at(11)).unwrap();
    assert_eq!(registry.len(), 2);
    registry.finalize(&at(11)).unwrap();
    registry.undo_to(&at(11)).unwrap();
}

#[test]
fn known_finality_remains_idempotent_after_history_eviction() {
    for limits in [
        Limits {
            max_undo_blocks: 1,
            ..Limits::default()
        },
        Limits {
            max_undo_pools: 1,
            ..Limits::default()
        },
    ] {
        let mut registry = Registry::new(seed(vec![]), limits).unwrap();
        registry.finalize(&at(10)).unwrap();
        commit(&mut registry, 11, vec![entry(1, &[])]);
        commit(&mut registry, 12, vec![entry(2, &[])]);
        registry.prepare_finalize(&at(10)).unwrap().commit();
        assert_eq!(registry.finalized(), Some(&at(10)));
        assert_eq!(registry.head(), &at(12));
        assert_eq!(registry.len(), 2);
        assert!(registry.finalize(&BlockRef { number: 10, hash: [99; 32] }).is_err());
        assert!(registry.finalize(&at(9)).is_err());
        // Retaining finality proof does not recreate an evicted undo target.
        assert!(registry.undo_to(&at(10)).is_err());
    }
}

#[test]
fn warm_anchor_applies_creations_atomically_and_records_the_gap() {
    let mut registry = registry();
    let identity = registry.seed_identity().clone();
    let creations = Pools { pools: vec![entry(2, &[])] };
    assert!(registry.prepare_anchor(at(9), &creations, true).is_err());
    assert!(registry.prepare_anchor(BlockRef { number: 10, hash: [99; 32] }, &creations, true).is_err());
    assert!(registry.prepare_anchor(at(20), &creations, false).is_err());
    {
        let prepared = registry.prepare_anchor(at(20), &creations, true).unwrap();
        assert_eq!(prepared.completeness(), Completeness::Partial);
        assert!(prepared.pool(&[2; 20]).is_some());
    }
    assert_eq!(registry.completeness(), Completeness::Complete);
    assert_eq!(registry.len(), 1);
    registry.prepare_anchor(at(20), &creations, true).unwrap().commit();
    assert_eq!(registry.seed_identity(), &identity);
    assert_eq!(registry.completeness(), Completeness::Partial);
    assert_eq!(registry.head(), &at(20));
    assert!(registry.prepare_anchor(at(21), &creations, true).is_err());
    commit(&mut registry, 21, vec![entry(3, &[])]);
    registry.undo_to(&at(20)).unwrap();
    assert_eq!(registry.len(), 2);
    assert!(registry.undo_to(&at(10)).is_err());
}

#[test]
fn same_height_anchor_and_explicit_partial_seed_never_upgrade_coverage() {
    let mut registry = registry();
    registry.anchor_at(at(10), false).unwrap();
    assert_eq!(registry.completeness(), Completeness::Complete);
    let mut registry = Registry::new(
        Seed {
            completeness: Completeness::Partial,
            ..seed(vec![])
        },
        Limits::default(),
    )
    .unwrap();
    commit(&mut registry, 11, vec![]);
    assert_eq!(registry.completeness(), Completeness::Partial);
    registry.undo_to(&at(10)).unwrap();
    assert!(registry.anchor_at(at(10), false).is_err());
}

#[test]
fn seed_fingerprint_binds_effective_metadata_and_provenance() {
    let base = seed(vec![entry(1, &[10]), entry(2, &[20, 21])]);
    let identity = Registry::new(base.clone(), Limits::default()).unwrap().seed_identity().clone();
    // Independently encoded from the documented length-prefixed layout.
    assert_eq!(
        hex::encode(identity.fingerprint),
        "926b923a5712927a56248c58d09e0e4bd2599c7fb0befad926964a9f8f04a664"
    );
    let mut reordered = base.clone();
    reordered.pools.reverse();
    reordered.pools.push(entry(1, &[99]));
    assert_eq!(identity, Registry::new(reordered, Limits::default()).unwrap().seed_identity().clone());
    let mut variants = Vec::new();
    let mut changed = base.clone();
    changed.chain = "other".into();
    variants.push(changed);
    let mut changed = base.clone();
    changed.revision = "other".into();
    variants.push(changed);
    let mut changed = base.clone();
    changed.at.number += 1;
    variants.push(changed);
    let mut changed = base.clone();
    changed.at.hash[0] ^= 1;
    variants.push(changed);
    let mut changed = base.clone();
    changed.completeness = Completeness::Partial;
    variants.push(changed);
    let mut changed = base.clone();
    changed.pools[0].pool.as_option_mut().unwrap().factory.clear();
    variants.push(changed);
    let mut changed = base;
    changed.pools[1].pool.as_option_mut().unwrap().tokens.reverse();
    variants.push(changed);
    for changed in variants {
        assert_ne!(
            identity.fingerprint,
            Registry::new(changed, Limits::default()).unwrap().seed_identity().fingerprint
        );
    }
}

fn topic(signature: &str) -> Vec<u8> {
    let mut digest = Keccak::v256();
    digest.update(signature.as_bytes());
    let mut bytes = [0; 32];
    digest.finalize(&mut bytes);
    bytes.to_vec()
}
fn word(bytes: &[u8]) -> Vec<u8> {
    let mut value = vec![0; 32 - bytes.len()];
    value.extend_from_slice(bytes);
    value
}
fn uint(value: u64) -> Vec<u8> {
    word(&value.to_be_bytes())
}
fn fixture() -> Block {
    let logs = vec![
        Log {
            address: vec![2; 20],
            topics: vec![topic("Swap(address,uint256,uint256,uint256,uint256,address)"), word(&[5; 20]), word(&[6; 20])],
            data: [uint(100), uint(0), uint(0), uint(90)].concat(),
            ordinal: 10,
            ..Default::default()
        },
        Log {
            address: vec![0xfa; 20],
            topics: vec![topic("PairCreated(address,address,address,uint256)"), word(&[10; 20]), word(&[11; 20])],
            data: [word(&[2; 20]), uint(1)].concat(),
            ordinal: 20,
            index: 1,
            ..Default::default()
        },
    ];
    Block {
        number: 11,
        transaction_traces: vec![TransactionTrace {
            status: TransactionTraceStatus::Succeeded.into(),
            hash: vec![7; 32],
            from: vec![8; 20],
            to: vec![9; 20],
            calls: vec![Call {
                index: 1,
                call_type: CallType::Call.into(),
                address: vec![9; 20],
                logs: logs.clone(),
                ..Default::default()
            }],
            receipt: TransactionReceipt { logs, ..Default::default() }.into(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

#[test]
fn actual_projectors_see_later_creation_before_earlier_swap_and_keep_exact_events() {
    let block = fixture();
    let creations = dex_pools::project(&block).unwrap();
    assert_eq!(creations.pools.len(), 1);
    let mut registry = Registry::new(seed(vec![]), Limits::default()).unwrap();
    let absent = dex_swaps::project(&block, &registry).unwrap();
    let prepared = registry.prepare_block(at(11), at(10).hash, &creations).unwrap();
    let events = dex_swaps::project(&block, &prepared).unwrap();
    let reference: HashMap<Vec<u8>, Pool> = creations
        .pools
        .iter()
        .map(|value| (hex::decode(&value.address).unwrap(), value.pool.as_option().unwrap().clone()))
        .collect();
    assert_eq!(events, dex_swaps::project(&block, &reference).unwrap());
    let swaps = |events: &dex_swaps::pb::Events| {
        events
            .transactions
            .iter()
            .flat_map(|tx| &tx.logs)
            .filter(|log| matches!(log.log, Some(dex_swaps::pb::log::Log::Swap(_))))
            .count()
    };
    assert_eq!(swaps(&absent), 0);
    assert_eq!(swaps(&events), 1);
    prepared.commit();
    assert!(registry.get_pool(&[2; 20]).is_some());
}

#[test]
fn actual_projector_failure_leaves_prepared_registry_uncommitted() {
    let mut block = fixture();
    let creations = dex_pools::project(&block).unwrap();
    block.transaction_traces[0].receipt = Default::default();
    let mut registry = Registry::new(seed(vec![]), Limits::default()).unwrap();
    {
        let prepared = registry.prepare_block(at(11), at(10).hash, &creations).unwrap();
        assert!(dex_swaps::project(&block, &prepared).is_err());
    }
    assert!(registry.is_empty());
    assert_eq!(registry.head(), &at(10));
}
