//! Released WASM, not a source-level reimplementation, is the independent oracle.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use substreams_database_change::pb::sf::substreams::sink::database::v1::{table_change::PrimaryKey, DatabaseChanges};
#[path = "../tests/support/legacy_wasm.rs"]
mod legacy_wasm;

/// A registered v0.8.0 panic: its message and the end of its location.
type Expected = (&'static str, &'static str);
/// ethabi 17.2.0 bounds-checks `offset + len` in 32-bit `usize` on wasm32.
const PEEK: &str = "ethabi-17.2.0/src/decoder.rs:71:17";
const TAKE: &str = "ethabi-17.2.0/src/decoder.rs:87:18";

/// What the released graph and the native port do with a case's block.
enum Expect {
    /// Both accept it with the same pool state, events and rows.
    Agree,
    /// Both fail it: `map_events` registers this panic and `project` errs.
    Reject(Expected),
    /// Accepted difference (owner, 2026-10-25): the released wasm32 build
    /// panics on a dynamic ABI offset or length whose end wraps 32-bit
    /// `usize`; the native readers reject the field and skip the log. The
    /// native result for the block equals the released graph's result for
    /// `control`, the same block without that log, and writes `tables`.
    NativeSkips {
        control: Box<Block>,
        store: Option<Expected>,
        swaps: Expected,
        tables: &'static [&'static str],
    },
}
struct Case {
    name: String,
    input: Block,
    seed: PoolMetadataMap,
    expect: Expect,
}
fn case(name: impl Into<String>, input: Block, seed: PoolMetadataMap) -> Case {
    Case {
        name: name.into(),
        input,
        seed,
        expect: Expect::Agree,
    }
}
fn hex(bytes: impl AsRef<[u8]>) -> String {
    substreams::Hex::encode(bytes.as_ref())
}
fn unhex(value: &str) -> Vec<u8> {
    if value.is_empty() {
        Vec::new()
    } else {
        substreams::Hex::decode(value).unwrap()
    }
}
fn pool_bytes(pools: &PoolMetadataMap) -> legacy_wasm::Pools {
    pools.iter().map(|(key, pool)| (hex(key), pool.encode_to_vec())).collect()
}
fn rows(changes: DatabaseChanges) -> Value {
    json!(changes
        .table_changes
        .into_iter()
        .map(|row| {
            let fields: BTreeMap<_, _> = row.fields.into_iter().map(|field| (field.name, field.value)).collect();
            json!({"table":row.table,"ordinal":row.ordinal,"fields":fields})
        })
        .collect::<Vec<_>>())
}
fn row_keys(changes: &DatabaseChanges) -> Value {
    json!(changes
        .table_changes
        .iter()
        .map(|row| {
            let key = match &row.primary_key {
                Some(PrimaryKey::Pk(value)) => json!({"single": value}),
                Some(PrimaryKey::CompositePk(value)) => json!({"composite": value.keys.iter().collect::<BTreeMap<_, _>>()}),
                None => Value::Null,
            };
            json!({"key": key, "operation": row.operation.to_i32()})
        })
        .collect::<Vec<_>>())
}
fn panic_json(panic: &legacy_wasm::Panic) -> Value {
    json!({"message": panic.message, "location": panic.location})
}
fn assert_panic(actual: Option<&legacy_wasm::Panic>, expected: Expected, what: &str) {
    let actual = actual.unwrap_or_else(|| panic!("{what}: expected the historical panic {expected:?}"));
    assert_eq!(actual.message, expected.0, "{what}");
    assert!(actual.location.ends_with(expected.1), "{what}: {}", actual.location);
}
fn pair_created(token0: u8, token1: u8) -> Log {
    log(
        &addr(FACTORY),
        vec![topic0("PairCreated(address,address,address,uint256)"), word(&addr(token0)), word(&addr(token1))],
        concat(&[word(&addr(V2)), uint(1)]),
    )
}
fn big<T: From<eth::BigInt>>(bytes: &[u8]) -> T {
    eth::BigInt { bytes: bytes.to_vec() }.into()
}
fn u32_word(value: u32) -> Vec<u8> {
    uint(u128::from(value))
}
/// A transaction whose CREATE subcall deploys `created`, as a Curve factory
/// deployment does.
fn with_create(mut trace: TransactionTrace, created: u8) -> TransactionTrace {
    trace.calls.push(Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        call_type: CallType::Create.into(),
        address: addr(created),
        ..Default::default()
    });
    trace
}
/// `block` without the logs whose topic 0 is `topic`, in calls and receipts.
fn without_logs(mut block: Block, topic: &[u8]) -> Block {
    for trace in &mut block.transaction_traces {
        let keep = |log: &Log| log.topics.first().map(Vec::as_slice) != Some(topic);
        for call in &mut trace.calls {
            call.logs.retain(keep);
        }
        if let Some(receipt) = trace.receipt.as_option_mut() {
            receipt.logs.retain(keep);
        }
    }
    block
}

// ── Malformed dynamic ABI data (offset and length words) ─────────────────────

fn cow_trade(offset: u32, len: u32, tail_words: usize) -> Log {
    let mut data = concat(&[
        word(&addr(T0)),
        word(&addr(T1)),
        uint(1000),
        uint(990),
        uint(5),
        u32_word(offset),
        u32_word(len),
    ]);
    data.extend(vec![0xab; 32 * tail_words]);
    log(
        &addr(0xc0),
        vec![topic0("Trade(address,address,address,uint256,uint256,uint256,bytes)"), word(&addr(SENDER))],
        data,
    )
}
fn balancer_pool_registered(pool: u8, offset: u32, tokens: &[Vec<u8>], len: u32) -> Log {
    let mut words = vec![u32_word(offset), uint(10u128.pow(16)), uint(0)];
    words.extend(std::iter::repeat_n(uint(0), 18));
    words.push(u32_word(len));
    for token in tokens {
        words.extend([word(token), uint(0), word(&addr(0)), uint(0)]);
    }
    log(
        &addr(VAULT),
        vec![
            topic0(
                "PoolRegistered(address,address,(address,uint8,address,bool)[],uint256,uint32,(address,address,address),\
                 (bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,address),(bool,bool,bool,bool))",
            ),
            word(&addr(pool)),
            word(&addr(BAL_FACTORY)),
        ],
        concat(&words),
    )
}
fn curve_plain_pool(offset: u32) -> Log {
    log(
        &addr(FACTORY),
        vec![topic0("PlainPoolDeployed(address[],uint256,uint256,address)")],
        concat(&[
            u32_word(offset),
            uint(100),
            uint(4_000_000),
            word(&addr(0x0d)),
            uint(2),
            word(&addr(T0)),
            word(&addr(T1)),
        ]),
    )
}
fn sunpump_legacy_create(name_offset: u32, name_len: u32) -> Log {
    log(
        &addr(0x1a),
        vec![topic0("TokenCreate(address,address,uint64,uint64,string,string)")],
        concat(&[
            word(&addr(SUN_TOKEN)),
            word(&addr(SENDER)),
            uint(1),
            uint(2),
            u32_word(name_offset),
            uint(256),
            u32_word(name_len),
            vec![0x41; 32],
            uint(1),
            vec![0x42; 32],
        ]),
    )
}

/// Pool creation for every collector, on an empty store, with a swap that
/// reads the created pool; first-writer order across transactions.
fn creation_cases() -> Vec<Case> {
    let one = |logs: Vec<Log>| block(vec![trx(&addr(ROOT), logs)]);
    let empty = PoolMetadataMap::new;
    let new_exchange = log(
        &addr(FACTORY),
        vec![topic0("NewExchange(address,address)"), word(&addr(T0)), word(&addr(V1))],
        vec![],
    );
    let v3_created = log(
        &addr(FACTORY),
        vec![
            topic0("PoolCreated(address,address,uint24,int24,address)"),
            word(&addr(T0)),
            word(&addr(T1)),
            uint(500),
        ],
        concat(&[int(-10), word(&addr(V3))]),
    );
    let aerodrome_created = log(
        &addr(FACTORY),
        vec![
            topic0("PoolCreated(address,address,bool,address,uint256)"),
            word(&addr(T0)),
            word(&addr(T1)),
            uint(1),
        ],
        concat(&[word(&addr(AERO)), uint(7)]),
    );
    let aerodrome_swap = log(
        &addr(AERO),
        vec![
            topic0("Swap(address,address,uint256,uint256,uint256,uint256)"),
            word(&addr(SENDER)),
            word(&addr(0x70)),
        ],
        concat(&[uint(0), uint(100), uint(90), uint(0)]),
    );
    let traderjoe_created = log(
        &addr(FACTORY),
        vec![
            topic0("LBPairCreated(address,address,uint256,address,uint256)"),
            word(&addr(T0)),
            word(&addr(T1)),
            uint(25),
        ],
        concat(&[word(&addr(TJ)), uint(3)]),
    );
    let mut cases = vec![
        case(
            "create_uniswap_v1",
            one(vec![new_exchange, v1_trade("TokenPurchase(address,uint256,uint256)", V1)]),
            empty(),
        ),
        case("create_uniswap_v3_and_kyber", one(vec![v3_created, v3_swap(V3, 100, -90)]), empty()),
        case(
            "create_uniswap_v4_empty_store",
            one(vec![v4_initialize(&v4_id()), v4_swap(&v4_id(), -100, 90)]),
            empty(),
        ),
        case("create_aerodrome", one(vec![aerodrome_created, aerodrome_swap]), empty()),
        case(
            "create_traderjoe",
            one(vec![traderjoe_created, traderjoe_swap(packed(100, 0), packed(0, 90))]),
            empty(),
        ),
        case(
            "create_balancer",
            one(vec![
                balancer_pool_registered(BAL, 21 * 32, &[addr(T0), addr(T1), addr(T2)], 3),
                balancer_swap(BAL),
            ]),
            empty(),
        ),
    ];
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
    for (name, creation) in [
        (
            "create_bancor_new_converter",
            log(
                &addr(FACTORY),
                vec![
                    topic0("NewConverter(uint16,address,address)"),
                    uint(3),
                    word(&addr(BANCOR)),
                    word(&addr(SENDER)),
                ],
                vec![],
            ),
        ),
        (
            "create_bancor_legacy_converter",
            log(
                &addr(FACTORY),
                vec![topic0("NewConverter(address,address)"), word(&addr(BANCOR)), word(&addr(SENDER))],
                vec![],
            ),
        ),
        (
            "create_bancor_features_addition",
            log(&addr(FACTORY), vec![topic0("FeaturesAddition(address,uint256)"), word(&addr(BANCOR))], uint(1)),
        ),
    ] {
        cases.push(case(name, one(vec![creation, conversion.clone()]), empty()));
    }
    let sunpump_v1 = log(
        &addr(0x1a),
        vec![topic0("TokenCreate(address,uint256,address)")],
        concat(&[word(&addr(SUN_TOKEN)), uint(9), word(&addr(SENDER))]),
    );
    cases.push(case("create_sunpump_v1", one(vec![sunpump_v1, sunpump_purchase(SUN_TOKEN)]), empty()));
    cases.push(case(
        "create_sunpump_legacy",
        one(vec![sunpump_legacy_create(192, 1), sunpump_purchase(SUN_TOKEN)]),
        empty(),
    ));
    // Curve factory events key the pool by the transaction's CREATE; the
    // exchange reads it in a later transaction of the block.
    let exchange = curve_exchange("TokenExchange(address,int128,uint256,int128,uint256)", int(0), int(1));
    let meta = log(
        &addr(FACTORY),
        vec![topic0("MetaPoolDeployed(address,address,uint256,uint256,address)")],
        concat(&[word(&addr(T0)), word(&addr(0xbb)), uint(100), uint(4_000_000), word(&addr(0x0d))]),
    );
    let mut crypto_words = vec![word(&addr(0x0b)), word(&addr(T0)), word(&addr(T1))];
    crypto_words.extend(std::iter::repeat_n(uint(1), 10));
    crypto_words.push(word(&addr(0x0d)));
    let crypto = log(
        &addr(FACTORY),
        vec![topic0(
            "CryptoPoolDeployed(address,address[2],uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,address)",
        )],
        concat(&crypto_words),
    );
    for (name, deployment) in [
        ("create_curve_plain", curve_plain_pool_deployed(uint(4_000_000))),
        ("create_curve_plain_truncated_fee", curve_plain_pool_deployed(uint(u128::from(u32::MAX) + 2))),
        ("create_curve_meta", meta),
        ("create_curve_crypto", crypto),
    ] {
        cases.push(case(
            name,
            block(vec![
                with_create(trx(&addr(FACTORY), vec![deployment]), CURVE),
                trx(&addr(ROOT), vec![exchange.clone()]),
            ]),
            empty(),
        ));
    }
    // A direct StableSwap deployment: a contract-creation transaction whose
    // input ends with the constructor arguments.
    let constructor = concat(&[
        word(&addr(0x0e)),
        word(&addr(T0)),
        word(&addr(T1)),
        word(&addr(T2)),
        word(&addr(0x1b)),
        uint(200),
        uint(4_000_000),
        uint(0),
    ]);
    let mut deployment = trx(&[], vec![]);
    deployment.input = [vec![0x60, 0x80, 0x60, 0x40], constructor].concat();
    deployment.calls[0].call_type = CallType::Create.into();
    deployment.calls[0].address = addr(CURVE);
    cases.push(case("create_curve_direct", block(vec![deployment, trx(&addr(ROOT), vec![exchange])]), empty()));
    let mut second_writer = pair_created(T2, T1);
    second_writer.address = addr(0xfb);
    cases.push(case(
        "create_first_writer_across_transactions",
        block(vec![
            trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90])]),
            trx(&addr(ROOT), vec![pair_created(T0, T1)]),
            trx(&addr(ROOT), vec![second_writer, v2_swap(V2, [0, 100, 90, 0])]),
        ]),
        empty(),
    ));
    cases
}

/// Fee and initialization events, transaction and call metadata across
/// several transactions, and a contract-creation transaction.
fn fee_and_transaction_cases() -> Vec<Case> {
    let launchpad = addr(0x1a);
    let fees = vec![
        log(
            &addr(CURVE),
            vec![topic0("CommitNewFee(uint256,uint256,uint256)"), uint(1)],
            concat(&[uint(4_000_000), uint(0)]),
        ),
        log(&addr(CURVE), vec![topic0("NewFee(uint256,uint256)")], concat(&[uint(3_000_000), uint(0)])),
        log(
            &addr(CURVE),
            vec![topic0("NewFee(uint256,uint256)")],
            concat(&[uint(u128::from(u32::MAX) + 7), uint(0)]),
        ),
        log(&addr(BAL), vec![topic0("SwapFeePercentageChanged(uint256)")], uint(3 * 10u128.pow(15))),
        // Above u64::MAX: 0 bps.
        log(&addr(BAL), vec![topic0("SwapFeePercentageChanged(uint256)")], vec![0xff; 32]),
        // No lookup names the pool: dropped.
        log(
            &addr(VAULT),
            vec![topic0("AggregateSwapFeePercentageChanged(address,uint256)"), word(&addr(BAL))],
            uint(10u128.pow(16)),
        ),
        log(
            &addr(BANCOR),
            vec![topic0("ConversionFeeUpdate(uint32,uint32)")],
            concat(&[uint(1000), uint(2000)]),
        ),
        log(&launchpad, vec![topic0("PurchaseFeeSet(uint256,uint256)")], concat(&[uint(100), uint(200)])),
        log(
            &launchpad,
            vec![topic0("SaleFeeSet(uint256,uint256)")],
            concat(&[uint(100), uint(u128::from(u32::MAX) + 3)]),
        ),
    ];
    let aggregate = log(
        &addr(VAULT),
        vec![topic0("AggregateSwapFeePercentageChanged(address,uint256)"), word(&addr(BAL))],
        uint(10u128.pow(16)),
    );
    // Uniswap V3 and V4 fees: `unwrap_or_default() as u32`, no block failure.
    let mut v3_created = log(
        &addr(FACTORY),
        vec![
            topic0("PoolCreated(address,address,uint24,int24,address)"),
            word(&addr(T0)),
            word(&addr(T1)),
            vec![0xff; 32],
        ],
        concat(&[int(10), word(&addr(0x5c))]),
    );
    let mut v4_truncated = v4_initialize(&[0x46; 32]);
    v4_truncated.data[..32].copy_from_slice(&uint(u128::from(u32::MAX) + 2));
    let mut v4_above_u64 = v4_initialize(&[0x47; 32]);
    v4_above_u64.data[..32].copy_from_slice(&[0xff; 32]);
    let mut cases = vec![
        case("fees_of_every_protocol", block(vec![trx(&addr(ROOT), fees)]), pools()),
        case(
            "fee_aggregate_named_by_swap",
            block(vec![trx(&addr(ROOT), vec![aggregate, balancer_swap(BAL)])]),
            pools(),
        ),
        case(
            "initialize_fee_conversions",
            block(vec![trx(&addr(ROOT), vec![v3_created.clone(), v4_truncated, v4_above_u64])]),
            pools(),
        ),
    ];
    v3_created.topics[3] = uint(u128::from(u32::MAX) + 3000);
    cases.push(case(
        "initialize_fee_truncated_above_u32",
        block(vec![trx(&addr(ROOT), vec![v3_created])]),
        pools(),
    ));

    // Transactions with events at positions 0 and 1 between a reverted one
    // and one without events; the second is a contract creation whose last
    // two logs come from a nested delegate call.
    let mut first = trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90])]);
    first.hash = vec![0x01; 32];
    first.index = 3;
    first.gas_price = big(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5]);
    first.value = big(&[0xff; 32]);
    let mut reverted = trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90])]);
    reverted.status = TransactionTraceStatus::Reverted.into();
    let mut no_events = trx(&addr(ROOT), vec![log(&addr(0x01), vec![], vec![])]);
    no_events.hash = vec![0x02; 32];
    let mut creation = trx(&[], vec![v3_swap(V3, -5, 7), pair_created(T0, T1), v2_swap(V2, [0, 100, 90, 0])]);
    creation.hash = vec![0x03; 32];
    creation.from = addr(0xf1);
    creation.nonce = u64::MAX;
    creation.gas_limit = u64::MAX;
    creation.gas_used = 1;
    creation.receipt.as_option_mut().unwrap().cumulative_gas_used = 999;
    for (index, log) in creation.receipt.as_option_mut().unwrap().logs.iter_mut().enumerate() {
        log.block_index = 100 + index as u32;
    }
    let root = &mut creation.calls[0];
    root.call_type = CallType::Create.into();
    root.caller = addr(0xf1);
    root.address = addr(0xc5);
    root.value = big(&[0x12, 0x34]);
    (root.begin_ordinal, root.end_ordinal, root.gas_limit, root.gas_consumed) = (5, 99, 77_777, 66_666);
    for log in &mut root.logs {
        log.block_index = 100 + (log.ordinal - 10) as u32;
    }
    let nested = root.logs.drain(1..).collect();
    creation.calls.push(Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        call_type: CallType::Delegate.into(),
        caller: addr(0xc5),
        address: addr(0xc6),
        begin_ordinal: 11,
        end_ordinal: 40,
        gas_limit: 5,
        gas_consumed: 4,
        value: big(&[]),
        logs: nested,
        ..Default::default()
    });
    cases.push(case(
        "multi_transaction_contract_creation",
        block(vec![first, reverted, no_events, creation]),
        pools(),
    ));
    let mut nested_reverted = trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90]), v2_swap(V2, [0, 100, 90, 0])]);
    let second = nested_reverted.calls[0].logs.drain(1..).collect();
    nested_reverted.calls.push(Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        call_type: CallType::Static.into(),
        address: addr(V2),
        state_reverted: true,
        logs: second,
        ..Default::default()
    });
    cases.push(case("nested_reverted_call", block(vec![nested_reverted]), pools()));
    let mut single_token = pools();
    single_token.get_mut(&addr(V3)).unwrap().tokens.truncate(1);
    cases.push(case(
        "v3_single_token_pool",
        block(vec![trx(&addr(ROOT), vec![v3_swap(V3, 100, -90)])]),
        single_token,
    ));
    cases
}

/// Dynamic offsets and lengths: valid words both implementations skip, and
/// the accepted wasm32 difference for each affected decoder.
fn malformed_abi_cases() -> Vec<Case> {
    let one = |logs: Vec<Log>| block(vec![trx(&addr(ROOT), logs)]);
    let mut cases = vec![
        case("cow_uid_length_without_wrap", one(vec![cow_trade(192, 0x7fff_ffff, 1)]), pools()),
        case("cow_uid_offset_without_wrap", one(vec![cow_trade(0x0001_0000, 4, 1)]), pools()),
        case("cow_uid_offset_unaligned", one(vec![cow_trade(193, 4, 2)]), pools()),
        case(
            "balancer_tokens_length_without_wrap",
            one(vec![balancer_pool_registered(0x77, 21 * 32, &[addr(0x60)], 0x7fff_ffff)]),
            pools(),
        ),
    ];
    let skipped = |name: &str, input: Block, store: Option<Expected>, swaps: Expected, tables| {
        let topic = input.transaction_traces.last().unwrap().calls[0].logs.last().unwrap().topics[0].clone();
        Case {
            name: name.into(),
            expect: Expect::NativeSkips {
                control: Box::new(without_logs(input.clone(), &topic)),
                store,
                swaps,
                tables,
            },
            input,
            seed: pools(),
        }
    };
    // 192 + 32 + 0xffff_ffff and 0xffff_ffe0 + 32 wrap; the slice panics.
    let length_wraps = ("slice index starts at 224 but ends at 223", TAKE);
    let offset_wraps = ("slice index starts at 4294967264 but ends at 0", PEEK);
    cases.extend([
        skipped("cow_uid_length_wraps", one(vec![cow_trade(192, u32::MAX, 1)]), None, length_wraps, &[]),
        skipped("cow_uid_offset_wraps", one(vec![cow_trade(0xffff_ffe0, 4, 1)]), None, offset_wraps, &[]),
        skipped(
            "balancer_tokens_offset_wraps",
            one(vec![balancer_pool_registered(0x77, 0xffff_ffe0, &[], 0)]),
            Some(offset_wraps),
            offset_wraps,
            &[],
        ),
        skipped(
            "curve_coins_offset_wraps",
            block(vec![with_create(trx(&addr(ROOT), vec![curve_plain_pool(0xffff_fff0)]), 0x7c)]),
            Some(("slice index starts at 4294967280 but ends at 16", PEEK)),
            ("slice index starts at 4294967280 but ends at 16", PEEK),
            &[],
        ),
        skipped(
            "sunpump_legacy_name_length_wraps",
            one(vec![sunpump_legacy_create(192, u32::MAX)]),
            Some(length_wraps),
            length_wraps,
            &[],
        ),
        skipped(
            "cow_uid_length_wraps_beside_v2_swap",
            block(vec![
                trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90])]),
                trx(&addr(ROOT), vec![cow_trade(192, u32::MAX, 1)]),
            ]),
            None,
            length_wraps,
            &["swaps", "blocks"],
        ),
    ]);
    cases
}

fn cases() -> Vec<Case> {
    let mut cases = vec![case("empty", block(vec![]), PoolMetadataMap::new())];
    let mut protocols = BTreeSet::new();
    for (index, (log, key, matched)) in pool_swap_cases().into_iter().enumerate() {
        protocols.extend(matched.iter().map(|p| format!("{p:?}")));
        for variant in ["known", "missing_pool", "empty_tokens", "malformed_tokens"] {
            let mut seed = pools();
            match variant {
                "missing_pool" => {
                    seed.remove(&key);
                }
                "empty_tokens" => seed.get_mut(&key).unwrap().tokens.clear(),
                "malformed_tokens" => {
                    for token in &mut seed.get_mut(&key).unwrap().tokens {
                        *token = vec![7; 19];
                    }
                }
                _ => {}
            }
            cases.push(case(format!("pool_{index}_{variant}"), block(vec![trx(&addr(ROOT), vec![log.clone()])]), seed));
        }
    }
    for (log, protocol) in stateless_swap_cases() {
        protocols.insert(format!("{protocol:?}"));
        cases.push(case(format!("{protocol:?}"), block(vec![trx(&addr(ROOT), vec![log])]), PoolMetadataMap::new()));
    }
    assert_eq!(protocols.len(), 14, "all supported swap protocols must be represented");
    let transaction = trx(&addr(ROOT), vec![v2_swap(V2, [100, 0, 0, 90])]);
    let mut receipt_only = transaction.clone();
    receipt_only.calls.clear();
    cases.push(case("receipt_only", block(vec![receipt_only]), pools()));
    let mut reverted = transaction.clone();
    reverted.calls[0].state_reverted = true;
    cases.push(case("reverted_call", block(vec![reverted]), pools()));
    let mut failed = transaction.clone();
    failed.status = TransactionTraceStatus::Failed.into();
    failed.receipt = Default::default();
    cases.push(case("failed_transaction", block(vec![failed]), pools()));
    let mut different = transaction.clone();
    different.receipt.as_option_mut().unwrap().logs[0].address = addr(UNKNOWN);
    cases.push(case("receipt_preload_differs_from_call", block(vec![different]), pools()));
    let mut maximum = v2_swap(V2, [0, 0, 0, 90]);
    maximum.data[..32].copy_from_slice(&[255; 32]);
    cases.push(case("uint256_max", block(vec![trx(&addr(ROOT), vec![maximum])]), pools()));
    for (index, amounts) in [[100, 100, 0, 90], [100, 0, 90, 0], [0, 0, 0, 0], [100, 0, 1, 90]].into_iter().enumerate() {
        cases.push(case(
            format!("ambiguous_v2_{index}"),
            block(vec![trx(&addr(ROOT), vec![v2_swap(V2, amounts)])]),
            pools(),
        ));
    }
    cases.push(case(
        "create_then_swap",
        block(vec![trx(&addr(ROOT), vec![pair_created(T0, T1), v2_swap(V2, [100, 0, 0, 90])])]),
        PoolMetadataMap::new(),
    ));
    cases.push(case(
        "create_first_writer_same_block",
        block(vec![trx(
            &addr(ROOT),
            vec![pair_created(T0, T1), pair_created(T1, T0), v2_swap(V2, [100, 0, 0, 90])],
        )]),
        PoolMetadataMap::new(),
    ));
    cases.push(case(
        "create_cannot_replace_seed",
        block(vec![trx(&addr(ROOT), vec![pair_created(T1, T0), v2_swap(V2, [100, 0, 0, 90])])]),
        pools(),
    ));
    cases.push(case(
        "v4_initialize_without_lookup",
        block(vec![trx(&addr(ROOT), vec![v4_initialize(&v4_id())])]),
        pools(),
    ));
    cases.push(case(
        "v4_initialize_with_swap_lookup",
        block(vec![trx(&addr(ROOT), vec![v4_initialize(&v4_id()), v4_swap(&v4_id(), 100, -90)])]),
        pools(),
    ));
    let all = pool_swap_cases()
        .into_iter()
        .map(|(log, _, _)| log)
        .chain(stateless_swap_cases().into_iter().map(|(log, _)| log))
        .collect();
    let mut metadata = trx(&addr(ROOT), all);
    metadata.index = 17;
    metadata.nonce = 7;
    metadata.gas_limit = 50_000;
    metadata.receipt.as_option_mut().unwrap().cumulative_gas_used = 123_456;
    metadata.calls[0].gas_consumed = 21_000;
    cases.push(case("all_protocols_metadata_order", block(vec![trx(&addr(ROOT), vec![]), metadata]), pools()));
    let mut missing = transaction;
    missing.receipt = Default::default();
    for (name, input, panic) in [
        (
            "missing_receipt",
            block(vec![missing]),
            (
                "called `Option::unwrap()` on a `None` value",
                "substreams-ethereum-core-0.11.1/src/block_view.rs:119:45",
            ),
        ),
        (
            "overflow_curve_fee",
            block(vec![trx(&addr(ROOT), vec![curve_plain_pool_deployed(vec![255; 32])])]),
            (
                "BigInt '115792089237316195423570985008687907853269984665640564039457584007913129639935' is too large to fit into u64",
                "substreams-0.7.6/src/scalar.rs:693:32",
            ),
        ),
    ] {
        let mut rejected = case(name, input, pools());
        rejected.expect = Expect::Reject(panic);
        cases.push(rejected);
    }
    cases.extend(creation_cases());
    cases.extend(fee_and_transaction_cases());
    cases.extend(malformed_abi_cases());
    cases
}

#[test]
fn released_v080_all_protocols_pool_lookup_and_row_parity() {
    let oracle = legacy_wasm::Oracle::new();
    let mut clock = substreams::pb::substreams::Clock {
        number: 1,
        id: "ab".repeat(32),
        ..Default::default()
    };
    clock.timestamp.get_or_insert_default().seconds = 1_760_000_042;
    clock.timestamp.get_or_insert_default().nanos = 7;
    let mut corpus = vec![];
    let mut outcomes = BTreeMap::<&str, usize>::new();
    for case in cases() {
        let input = case.input.encode_to_vec();
        let seed = pool_bytes(&case.seed);
        // The released graph's output that the native result must equal.
        let reference = match &case.expect {
            Expect::NativeSkips { control, .. } => control.encode_to_vec(),
            _ => input.clone(),
        };
        let old_store = oracle.store(&reference, seed.clone()).expect("unexpected pool-store rejection");
        let mut native_pools = case.seed.clone();
        for created in dex_pools::project(&case.input).unwrap().pools {
            native_pools
                .entry(substreams::Hex::decode(&created.address).unwrap())
                .or_insert(created.pool.into_option().unwrap());
        }
        assert_eq!(pool_bytes(&native_pools), old_store.pools, "pool state {}", case.name);
        let old = oracle.swaps(&reference, old_store.pools.clone());
        let native = project(&case.input, &native_pools);
        let (outcome, panics) = match &case.expect {
            Expect::Agree => {
                assert!(old.is_ok(), "historical rejection {}", case.name);
                assert!(native.is_ok(), "native rejection {}", case.name);
                ("agree", Value::Null)
            }
            Expect::Reject(expected) => {
                assert_panic(old.as_ref().err(), *expected, &case.name);
                assert!(native.is_err(), "native rejection {}", case.name);
                ("rejected", json!({"store_pools": null, "map_events": panic_json(old.as_ref().err().unwrap())}))
            }
            Expect::NativeSkips { store, swaps, tables, .. } => {
                assert!(old.is_ok(), "historical control rejection {}", case.name);
                let native = native.as_ref().unwrap_or_else(|e| panic!("native rejection {}: {e}", case.name));
                let written: Vec<_> = evm_dex::changes(&clock, native, evm_dex::Encoding::Hex)
                    .unwrap()
                    .table_changes
                    .into_iter()
                    .map(|row| row.table)
                    .collect();
                assert_eq!(written, *tables, "native tables {}", case.name);
                let historical_store = oracle.store(&input, seed.clone());
                match store {
                    Some(expected) => assert_panic(historical_store.as_ref().err(), *expected, &case.name),
                    None => assert_eq!(historical_store.as_ref().unwrap().pools, old_store.pools, "pool state {}", case.name),
                }
                // map_events fails the block even with the control's pools.
                let historical_swaps = oracle.swaps(&input, old_store.pools.clone());
                assert_panic(historical_swaps.as_ref().err(), *swaps, &case.name);
                let panics = json!({
                    "store_pools": historical_store.as_ref().err().map(panic_json),
                    "map_events": panic_json(historical_swaps.as_ref().err().unwrap()),
                });
                ("historical_rejects_native_skips_log", panics)
            }
        };
        *outcomes.entry(outcome).or_default() += 1;
        let mut formatted = vec![];
        if let (Ok(old), Ok(native)) = (&old, &native) {
            assert_eq!(pb::Events::decode_from_slice(&old.output).unwrap(), *native, "events {}", case.name);
            for (name, encoding) in [("hex", evm_dex::Encoding::Hex), ("tron_base58", evm_dex::Encoding::TronBase58)] {
                let raw = oracle.rows(&clock.encode_to_vec(), &old.output, name.as_bytes()).unwrap().output;
                let historical_changes = DatabaseChanges::decode_from_slice(&raw).unwrap();
                let actual_changes = evm_dex::changes(&clock, native, encoding).unwrap();
                // The bounded entry point, at the limits pinax-streams #74 uses.
                let bounded = evm_dex::changes_bounded(&clock, native, encoding, 65_536, evm_dex::MAX_BYTES_BOUND).unwrap();
                assert_eq!(bounded, actual_changes, "bounded rows {} {name}", case.name);
                let keys = row_keys(&historical_changes);
                assert_eq!(keys, row_keys(&actual_changes), "row keys {} {name}", case.name);
                let historical = rows(historical_changes);
                let actual = rows(actual_changes);
                assert_eq!(historical, actual, "rows {} {name}", case.name);
                formatted.push(json!({"encoding":name,"oracle_database_changes_hex":hex(&raw),"rows":historical,"row_keys":keys}));
            }
        }
        let pool_json = |p: legacy_wasm::Pools| p.into_iter().map(|(k, v)| (k, hex(v))).collect::<BTreeMap<_, _>>();
        let mut entry = json!({"name":case.name,"block_hex":hex(&input),"block_sha256":hex(Sha256::digest(&input)),"seed":pool_json(seed),
            "outcome":outcome,"rejected":matches!(case.expect, Expect::Reject(_)),"historical_panics":panics,
            "post_store":pool_json(old_store.pools),"oracle_events_hex":old.as_ref().ok().map(|old|hex(&old.output)),"formatted":formatted});
        if let Expect::NativeSkips { control, .. } = &case.expect {
            let control = control.encode_to_vec();
            entry["control_block_hex"] = json!(hex(&control));
            entry["control_block_sha256"] = json!(hex(Sha256::digest(&control)));
        }
        corpus.push(entry);
    }
    let expected_outcomes = BTreeMap::from([("agree", 99), ("rejected", 2), ("historical_rejects_native_skips_log", 6)]);
    assert_eq!(outcomes, expected_outcomes, "case outcomes; update the fixture README with them");
    let result = json!({"schema_version":2,"package_sha256":"761fe2c6b2d46369781730d53962f0a348fd0df0faf3943f5979e8a2316d11d2",
        "clock_hex":hex(clock.encode_to_vec()),"cases":corpus});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy-v0.8.0/corpus.json");
    if std::env::var_os("UPDATE_EVM_DEX_V080_CORPUS").as_deref() == Some(std::ffi::OsStr::new("1")) {
        std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&result).unwrap())).unwrap();
    }
    let mut saved: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut current = result;
    // The DB map's field/key serialization order is not the row contract.
    // Preserve frozen raw bytes, but compare their decoded rows, keys and
    // operations: an edited frozen operation or key fails here.
    for corpus in [&mut saved, &mut current] {
        for case in corpus["cases"].as_array_mut().unwrap() {
            let name = case["name"].clone();
            for output in case["formatted"].as_array_mut().unwrap() {
                let raw = output.as_object_mut().unwrap().remove("oracle_database_changes_hex").unwrap();
                let changes = DatabaseChanges::decode_from_slice(&unhex(raw.as_str().unwrap())).unwrap();
                assert_eq!(row_keys(&changes), output["row_keys"], "frozen row keys {name}");
                assert_eq!(rows(changes), output["rows"], "frozen rows {name}");
            }
        }
    }
    assert_eq!(saved, current, "frozen oracle inputs/results changed");
}
