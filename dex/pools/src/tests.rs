//! Synthetic-log tests of every pool collector and of `project`. Expected
//! values follow the v0.8.0 (`9663b25`) collectors line by line.

use super::*;
use crate::test_support::*;
use substreams::store::DeltaProto;
use substreams_ethereum::pb::eth::v2::{Call, TransactionTraceStatus};

const FACTORY: u8 = 0xfa;
const POOL: u8 = 0x50;
const TOKEN0: u8 = 0x10;
const TOKEN1: u8 = 0x11;
const TOKEN2: u8 = 0x12;

fn pool(tokens: Vec<Vec<u8>>, factory: Vec<u8>) -> Pool {
    Pool { tokens, factory }
}

/// The single pool `project` emits for one log in one call.
fn single(log: Log) -> PoolCreated {
    let pools = project(&block(vec![trx(&addr(0xee), vec![log])])).unwrap().pools;
    assert_eq!(pools.len(), 1, "{pools:?}");
    pools.into_iter().next().unwrap()
}

fn assert_pool(created: PoolCreated, address: &[u8], expected: Pool) {
    assert_eq!(created.address, Hex::encode(address));
    assert_eq!(created.pool.as_option(), Some(&expected));
}

fn uniswap_v2_pair_created() -> Log {
    log(
        &addr(FACTORY),
        vec![topic0("PairCreated(address,address,address,uint256)"), word(&addr(TOKEN0)), word(&addr(TOKEN1))],
        concat(&[word(&addr(POOL)), uint(1)]),
    )
}

#[test]
fn uniswap_v1_new_exchange() {
    let log = log(
        &addr(FACTORY),
        vec![topic0("NewExchange(address,address)"), word(&addr(TOKEN0)), word(&addr(POOL))],
        vec![],
    );
    assert_pool(single(log), &addr(POOL), pool(vec![addr(TOKEN0)], addr(FACTORY)));
}

#[test]
fn uniswap_v2_pair_created_keys_the_pair() {
    assert_pool(
        single(uniswap_v2_pair_created()),
        &addr(POOL),
        pool(vec![addr(TOKEN0), addr(TOKEN1)], addr(FACTORY)),
    );
}

fn v3_shaped_pool_created() -> Log {
    log(
        &addr(FACTORY),
        vec![
            topic0("PoolCreated(address,address,uint24,int24,address)"),
            word(&addr(TOKEN0)),
            word(&addr(TOKEN1)),
            uint(3000),
        ],
        concat(&[uint(60), word(&addr(POOL))]),
    )
}

#[test]
fn uniswap_v3_pool_created() {
    assert_pool(
        single(v3_shaped_pool_created()),
        &addr(POOL),
        pool(vec![addr(TOKEN0), addr(TOKEN1)], addr(FACTORY)),
    );
}

#[test]
fn kyber_elastic_pool_created_shares_the_v3_signature_and_entry() {
    // Kyber Elastic's PoolCreated(token0, token1, swapFeeUnits, tickDistance,
    // pool) has the V3 signature, so the V3 collector, tried first, takes it;
    // the entry is the same either way.
    let log = v3_shaped_pool_created();
    assert!(uniswap::collect_uniswap_v3(&log).is_some());
    let kyber = kyber::collect_kyber_elastic(&log).unwrap();
    assert_eq!(
        (kyber.address, kyber.tokens, kyber.factory),
        (addr(POOL), vec![addr(TOKEN0), addr(TOKEN1)], addr(FACTORY))
    );
}

#[test]
fn uniswap_v4_initialize_keys_the_pool_id() {
    let id = vec![0x44; 32];
    let log = log(
        &addr(FACTORY),
        vec![
            topic0("Initialize(bytes32,address,address,uint24,int24,address,uint160,int24)"),
            id.clone(),
            word(&addr(TOKEN0)),
            word(&addr(TOKEN1)),
        ],
        concat(&[uint(500), uint(10), word(&addr(0)), uint(1 << 96), uint(0)]),
    );
    assert_pool(single(log), &id, pool(vec![addr(TOKEN0), addr(TOKEN1)], addr(FACTORY)));
}

#[test]
fn aerodrome_pool_created() {
    let log = log(
        &addr(FACTORY),
        vec![
            topic0("PoolCreated(address,address,bool,address,uint256)"),
            word(&addr(TOKEN0)),
            word(&addr(TOKEN1)),
            uint(1),
        ],
        concat(&[word(&addr(POOL)), uint(7)]),
    );
    assert_pool(single(log), &addr(POOL), pool(vec![addr(TOKEN0), addr(TOKEN1)], addr(FACTORY)));
}

#[test]
fn traderjoe_lb_pair_created() {
    let log = log(
        &addr(FACTORY),
        vec![
            topic0("LBPairCreated(address,address,uint256,address,uint256)"),
            word(&addr(TOKEN0)),
            word(&addr(TOKEN1)),
            uint(25),
        ],
        concat(&[word(&addr(POOL)), uint(3)]),
    );
    assert_pool(single(log), &addr(POOL), pool(vec![addr(TOKEN0), addr(TOKEN1)], addr(FACTORY)));
}

/// A Balancer V3 `PoolRegistered` log for `tokens`, emitted by the vault.
pub(crate) fn balancer_pool_registered(vault: &[u8], tokens: &[Vec<u8>]) -> Log {
    // Head: tokenConfig offset, swapFeePercentage, pauseWindowEndTime,
    // roleAccounts (3 words), hooksConfig (11 words), liquidityManagement
    // (4 words); then the tokenConfig tail of 4-word tuples.
    let mut words = vec![uint(21 * 32), uint(10u128.pow(16)), uint(0)];
    words.extend(std::iter::repeat_n(uint(0), 18));
    words.push(uint(tokens.len() as u128));
    for token in tokens {
        words.extend([word(token), uint(0), word(&addr(0)), uint(0)]);
    }
    log(
        vault,
        vec![
            topic0(
                "PoolRegistered(address,address,(address,uint8,address,bool)[],uint256,uint32,(address,address,address),\
                 (bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,address),(bool,bool,bool,bool))",
            ),
            word(&addr(POOL)),
            word(&addr(FACTORY)),
        ],
        concat(&words),
    )
}

#[test]
fn balancer_v3_pool_registered_takes_the_event_factory() {
    let log = balancer_pool_registered(&addr(0xba), &[addr(TOKEN0), addr(TOKEN1), addr(TOKEN2)]);
    assert_pool(single(log), &addr(POOL), pool(vec![addr(TOKEN0), addr(TOKEN1), addr(TOKEN2)], addr(FACTORY)));
}

#[test]
fn bancor_converters_and_features_have_no_tokens() {
    let new_converter = log(
        &addr(FACTORY),
        vec![topic0("NewConverter(uint16,address,address)"), uint(1), word(&addr(POOL)), word(&addr(0x0e))],
        vec![],
    );
    let legacy_converter = log(
        &addr(FACTORY),
        vec![topic0("NewConverter(address,address)"), word(&addr(POOL)), word(&addr(0x0e))],
        vec![],
    );
    let features = log(&addr(FACTORY), vec![topic0("FeaturesAddition(address,uint256)"), word(&addr(POOL))], uint(1));
    for log in [new_converter, legacy_converter, features] {
        assert_pool(single(log), &addr(POOL), pool(vec![], addr(FACTORY)));
    }
}

#[test]
fn sunpump_token_creations_key_the_token() {
    let v1 = log(
        &addr(FACTORY),
        vec![topic0("TokenCreate(address,uint256,address)")],
        concat(&[word(&addr(POOL)), uint(9), word(&addr(0x0c))]),
    );
    // Two empty strings: head offsets 192 and 224, each tail a zero length.
    let legacy = log(
        &addr(FACTORY),
        vec![topic0("TokenCreate(address,address,uint64,uint64,string,string)")],
        concat(&[word(&addr(POOL)), word(&addr(0x0c)), uint(1), uint(2), uint(192), uint(224), uint(0), uint(0)]),
    );
    for log in [v1, legacy] {
        assert_pool(single(log), &addr(POOL), pool(vec![], addr(FACTORY)));
    }
}

/// A transaction calling a Curve factory that CREATEs `pool` and logs `log`.
fn curve_factory_trx(log: Log, created: Option<&[u8]>) -> TransactionTrace {
    let mut trx = trx(&addr(FACTORY), vec![log]);
    if let Some(created) = created {
        trx.calls.push(Call {
            index: 2,
            parent_index: 1,
            depth: 1,
            call_type: CallType::Create.into(),
            address: created.to_vec(),
            ..Default::default()
        });
    }
    trx
}

fn curve_plain_pool_deployed() -> Log {
    log(
        &addr(FACTORY),
        vec![topic0("PlainPoolDeployed(address[],uint256,uint256,address)")],
        concat(&[
            uint(128),
            uint(100),
            uint(4_000_000),
            word(&addr(0x0d)),
            uint(2),
            word(&addr(TOKEN0)),
            word(&addr(TOKEN1)),
        ]),
    )
}

#[test]
fn curve_factory_events_key_the_created_contract() {
    let meta = log(
        &addr(FACTORY),
        vec![topic0("MetaPoolDeployed(address,address,uint256,uint256,address)")],
        concat(&[word(&addr(TOKEN0)), word(&addr(0xbb)), uint(100), uint(4_000_000), word(&addr(0x0d))]),
    );
    let mut crypto_words = vec![word(&addr(0x0b)), word(&addr(TOKEN0)), word(&addr(TOKEN1))];
    crypto_words.extend(std::iter::repeat_n(uint(1), 10));
    crypto_words.push(word(&addr(0x0d)));
    let crypto = log(
        &addr(FACTORY),
        vec![topic0(
            "CryptoPoolDeployed(address,address[2],uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,address)",
        )],
        concat(&crypto_words),
    );
    let cases = [
        (curve_plain_pool_deployed(), vec![addr(TOKEN0), addr(TOKEN1)]),
        (meta, vec![addr(TOKEN0)]),
        (crypto, vec![addr(TOKEN0), addr(TOKEN1)]),
    ];
    for (log, tokens) in cases {
        let pools = project(&block(vec![curve_factory_trx(log.clone(), Some(&addr(POOL)))])).unwrap().pools;
        assert_eq!(pools.len(), 1);
        assert_pool(pools.into_iter().next().unwrap(), &addr(POOL), pool(tokens, addr(FACTORY)));

        // Without a CREATE in the transaction there is no pool address.
        assert!(project(&block(vec![curve_factory_trx(log, None)])).unwrap().pools.is_empty());
    }
}

#[test]
fn curve_direct_deployment_reads_the_constructor_tail() {
    let constructor = concat(&[
        word(&addr(0x0e)),
        word(&addr(TOKEN0)),
        word(&addr(TOKEN1)),
        word(&addr(TOKEN2)),
        word(&addr(0x1b)),
        uint(200),
        uint(4_000_000),
        uint(0),
    ]);
    let mut deploy = trx(&[], vec![]);
    deploy.input = [vec![0x60, 0x80, 0x60, 0x40], constructor].concat();
    deploy.calls[0].call_type = CallType::Create.into();
    deploy.calls[0].address = addr(POOL);

    let pools = project(&block(vec![deploy.clone()])).unwrap().pools;
    assert_eq!(pools.len(), 1);
    assert_pool(
        pools.into_iter().next().unwrap(),
        &addr(POOL),
        pool(vec![addr(TOKEN0), addr(TOKEN1), addr(TOKEN2)], vec![]),
    );

    // A call input too short for the constructor tail is not a pool.
    deploy.input.truncate(100);
    assert!(project(&block(vec![deploy])).unwrap().pools.is_empty());
}

#[test]
fn unrelated_and_malformed_logs_are_ignored() {
    let mut wrong_length = uniswap_v2_pair_created();
    wrong_length.data.push(0);
    let mut wrong_topics = uniswap_v2_pair_created();
    wrong_topics.topics.pop();
    let unrelated = log(&addr(FACTORY), vec![topic0("Transfer(address,address,uint256)")], uint(1));
    for log in [wrong_length, wrong_topics, unrelated] {
        assert!(project(&block(vec![trx(&addr(0xee), vec![log])])).unwrap().pools.is_empty());
    }
}

#[test]
fn first_write_wins_within_a_block_and_failed_transactions_are_skipped() {
    let mut other_factory = uniswap_v2_pair_created();
    other_factory.address = addr(0xfb);
    let mut failed = trx(&addr(0xee), vec![v3_shaped_pool_created()]);
    failed.status = TransactionTraceStatus::Reverted.into();

    let input = block(vec![
        failed,
        trx(&addr(0xee), vec![uniswap_v2_pair_created()]),
        trx(&addr(0xee), vec![other_factory]),
    ]);
    let pools = project(&input).unwrap().pools;

    assert_eq!(pools.len(), 1);
    assert_eq!(pools[0].pool.factory, addr(FACTORY));
    assert_eq!(pools[0].pool.tokens, vec![addr(TOKEN0), addr(TOKEN1)]);
}

#[test]
fn reverted_call_logs_are_skipped_and_receipts_are_read_only_without_calls() {
    let mut reverted = trx(&addr(0xee), vec![uniswap_v2_pair_created()]);
    reverted.calls[0].state_reverted = true;
    assert!(project(&block(vec![reverted])).unwrap().pools.is_empty());

    let mut receipt_only = trx(&addr(0xee), vec![uniswap_v2_pair_created()]);
    receipt_only.calls.clear();
    assert_eq!(project(&block(vec![receipt_only.clone()])).unwrap().pools.len(), 1);

    // v0.8.0 panicked here; the port fails the block with an error.
    receipt_only.receipt = buffa::MessageField::none();
    assert!(project(&block(vec![receipt_only])).is_err());
}

#[test]
fn empty_pool_addresses_are_not_stored() {
    // A direct deployment whose CREATE call has no address decodes, but
    // v0.8.0's `set_pool` skipped empty keys.
    let mut deploy = trx(&[], vec![]);
    deploy.calls[0].call_type = CallType::Create.into();
    deploy.calls[0].address.clear();
    deploy.input = vec![0; 300];
    assert!(curvefi::collect_curvefi_init(&deploy).is_some());
    assert!(entries(&block(vec![deploy])).unwrap().is_empty());
}

fn delta(operation: Operation, key: &str, value: Pool) -> DeltaProto<Pool> {
    DeltaProto {
        operation,
        ordinal: 0,
        key: key.to_string(),
        old_value: Pool::default(),
        new_value: value,
    }
}

#[test]
fn map_pools_emits_only_create_deltas() {
    let created = pool(vec![addr(TOKEN0)], addr(FACTORY));
    let deltas = Deltas {
        deltas: vec![
            delta(Operation::Create, "50", created.clone()),
            delta(Operation::Update, "51", created.clone()),
            delta(Operation::Delete, "52", created.clone()),
        ],
    };
    let pools = pools_from_deltas(deltas).pools;
    assert_eq!(pools.len(), 1);
    assert_eq!(pools[0].address, "50");
    assert_eq!(pools[0].pool.as_option(), Some(&created));
}

/// `project` equals `map_pools` over the deltas `store_pools` produces when
/// the store held none of the block's pools: a `set_if_not_exists` write
/// creates a key once, in write order.
#[test]
fn project_matches_the_store_and_map_modules_on_an_empty_store() {
    let mut other_factory = uniswap_v2_pair_created();
    other_factory.address = addr(0xfb);
    let input = block(vec![
        trx(
            &addr(0xee),
            vec![uniswap_v2_pair_created(), balancer_pool_registered(&addr(0xba), &[addr(TOKEN2)])],
        ),
        curve_factory_trx(curve_plain_pool_deployed(), Some(&addr(0x51))),
        trx(&addr(0xee), vec![other_factory]),
    ]);

    let mut stored = HashSet::new();
    let deltas = entries(&input)
        .unwrap()
        .into_iter()
        .filter_map(|entry| {
            let key = Hex::encode(&entry.address);
            stored
                .insert(key.clone())
                .then(|| delta(Operation::Create, &key, pool(entry.tokens, entry.factory)))
        })
        .collect();

    let projected = project(&input).unwrap();
    assert_eq!(projected.pools.len(), 2, "the Balancer and V2 pools share an address");
    assert_eq!(projected, pools_from_deltas(Deltas { deltas }));
}
