//! Released WASM, not a source-level reimplementation, is the independent oracle.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use substreams_database_change::pb::sf::substreams::sink::database::v1::{table_change::PrimaryKey, DatabaseChanges};
#[path = "../tests/support/legacy_wasm.rs"]
mod legacy_wasm;

struct Case {
    name: String,
    input: Block,
    seed: PoolMetadataMap,
    reject: bool,
}
fn case(name: impl Into<String>, input: Block, seed: PoolMetadataMap) -> Case {
    Case {
        name: name.into(),
        input,
        seed,
        reject: false,
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
fn pair_created(token0: u8, token1: u8) -> Log {
    log(
        &addr(FACTORY),
        vec![topic0("PairCreated(address,address,address,uint256)"), word(&addr(token0)), word(&addr(token1))],
        concat(&[word(&addr(V2)), uint(1)]),
    )
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
    for (name, input) in [
        ("missing_receipt", block(vec![missing])),
        (
            "overflow_curve_fee",
            block(vec![trx(&addr(ROOT), vec![curve_plain_pool_deployed(vec![255; 32])])]),
        ),
    ] {
        let mut rejected = case(name, input, pools());
        rejected.reject = true;
        cases.push(rejected);
    }
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
    for case in cases() {
        let input = case.input.encode_to_vec();
        let seed = pool_bytes(&case.seed);
        let old_store = oracle.store(&input, seed.clone()).expect("unexpected pool-store rejection");
        let mut native_pools = case.seed.clone();
        for created in dex_pools::project(&case.input).unwrap().pools {
            native_pools
                .entry(substreams::Hex::decode(&created.address).unwrap())
                .or_insert(created.pool.into_option().unwrap());
        }
        assert_eq!(pool_bytes(&native_pools), old_store.pools, "pool state {}", case.name);
        let old = oracle.swaps(&input, old_store.pools.clone());
        let native = project(&case.input, &native_pools);
        assert_eq!(native.is_err(), case.reject, "native rejection {}", case.name);
        assert_eq!(old.is_none(), case.reject, "historical rejection {}", case.name);
        let mut formatted = vec![];
        if let (Some(old), Ok(native)) = (&old, &native) {
            assert_eq!(pb::Events::decode_from_slice(&old.output).unwrap(), *native, "events {}", case.name);
            for (name, encoding) in [("hex", evm_dex::Encoding::Hex), ("tron_base58", evm_dex::Encoding::TronBase58)] {
                let raw = oracle.rows(&clock.encode_to_vec(), &old.output, name.as_bytes()).unwrap().output;
                let historical_changes = DatabaseChanges::decode_from_slice(&raw).unwrap();
                let actual_changes = evm_dex::changes(&clock, native, encoding).unwrap();
                assert_eq!(row_keys(&historical_changes), row_keys(&actual_changes), "row keys {} {name}", case.name);
                let historical = rows(historical_changes);
                let actual = rows(actual_changes);
                assert_eq!(historical, actual, "rows {} {name}", case.name);
                formatted.push(json!({"encoding":name,"oracle_database_changes_hex":hex(&raw),"rows":historical}));
            }
        }
        let pool_json = |p: legacy_wasm::Pools| p.into_iter().map(|(k, v)| (k, hex(v))).collect::<BTreeMap<_, _>>();
        corpus.push(
            json!({"name":case.name,"block_hex":hex(&input),"block_sha256":hex(Sha256::digest(&input)),"seed":pool_json(seed),
            "post_store":pool_json(old_store.pools),"rejected":case.reject,
            "oracle_events_hex":old.as_ref().map(|old|hex(&old.output)),"formatted":formatted}),
        );
    }
    let result = json!({"schema_version":1,"package_sha256":"761fe2c6b2d46369781730d53962f0a348fd0df0faf3943f5979e8a2316d11d2",
        "clock_hex":hex(clock.encode_to_vec()),"cases":corpus});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy-v0.8.0/corpus.json");
    if std::env::var_os("UPDATE_EVM_DEX_V080_CORPUS").as_deref() == Some(std::ffi::OsStr::new("1")) {
        std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&result).unwrap())).unwrap();
    }
    let mut saved: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut current = result;
    // The DB map's field/key serialization order is not the row contract.
    // Preserve frozen raw bytes, but compare their decoded row fields/order.
    for corpus in [&mut saved, &mut current] {
        for case in corpus["cases"].as_array_mut().unwrap() {
            for output in case["formatted"].as_array_mut().unwrap() {
                let raw = output.as_object_mut().unwrap().remove("oracle_database_changes_hex").unwrap();
                assert_eq!(rows(DatabaseChanges::decode_from_slice(&unhex(raw.as_str().unwrap())).unwrap()), output["rows"]);
            }
        }
    }
    assert_eq!(saved, current, "frozen oracle inputs/results changed");
}
