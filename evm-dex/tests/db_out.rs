//! `db_out` through the crate's public API: v0.8.0's rows for every table,
//! both address encodings, the errors that replace v0.8.0's panics, the
//! manifest wiring and the embedded v0.8.0 schema.
use buffa_types::google::protobuf::Timestamp;
use evm_dex::{changes, pb, Encoding};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use substreams::{pb::substreams::Clock, Hex};
use substreams_database_change::pb::sf::substreams::sink::database::v1::{
    field::UpdateOp,
    table_change::{Operation, PrimaryKey},
    TableChange,
};

const MANIFEST: &str = include_str!("../substreams.yaml");
/// `2^256 - 1`, the largest value of the `UInt256` amount columns.
const MAX_UINT256: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
const BLOCK_HASH: &str = "0xabababababababababababababababababababababababababababababababab";

/// The columns that hold an address, and so follow the encoding.
const ADDRESS_COLUMNS: [&str; 10] = [
    "tx_from",
    "tx_to",
    "log_address",
    "call_caller",
    "call_address",
    "factory",
    "pool",
    "user",
    "input_contract",
    "output_contract",
];

fn clock() -> Clock {
    Clock {
        id: "ab".repeat(32),
        number: 23_000_000,
        timestamp: buffa::MessageField::some(Timestamp {
            seconds: 1_760_000_042,
            nanos: 7,
            ..Default::default()
        }),
    }
}

fn address(byte: u8) -> Vec<u8> {
    vec![byte; 20]
}

fn hex20(byte: u8) -> String {
    format!("0x{}", format!("{byte:02x}").repeat(20))
}

fn call() -> pb::Call {
    pb::Call {
        index: 3,
        begin_ordinal: 10,
        end_ordinal: 20,
        caller: address(0xca),
        address: address(0xad),
        value: "5".into(),
        gas_consumed: 21_000,
        gas_limit: 30_000,
        depth: 2,
        parent_index: 1,
        call_type: pb::CallType::Delegate.into(),
    }
}

fn log(event: Option<pb::log::Log>, call: Option<pb::Call>) -> pb::Log {
    pb::Log {
        address: address(0x10),
        ordinal: 77,
        topics: vec![vec![0xdd; 32], vec![0x01; 32]],
        data: vec![0x01, 0x02],
        call: call.map(buffa::MessageField::some).unwrap_or_default(),
        block_index: 5,
        log: event,
    }
}

fn swap() -> pb::log::Log {
    pb::log::Log::from(pb::Swap {
        protocol: pb::Protocol::UniswapV3.into(),
        factory: address(0xfa),
        pool: address(0x10),
        user: address(0x05),
        input_token: address(0x70),
        input_amount: "1000".into(),
        output_token: address(0x71),
        output_amount: MAX_UINT256.into(),
    })
}

fn swap_fee() -> pb::log::Log {
    pb::log::Log::from(pb::SwapFee {
        protocol: pb::Protocol::Curvefi.into(),
        factory: address(0xfb),
        pool: address(0x11),
        fee: 4_000_000,
    })
}

fn initialize() -> pb::log::Log {
    pb::log::Log::from(pb::Initialize {
        protocol: pb::Protocol::UniswapV2.into(),
        factory: address(0xfc),
        pool: address(0x12),
    })
}

/// Two transactions: a swap, a log without an event, then a fee setting
/// without call metadata; and a contract creation that initializes a pool,
/// with a call type outside the enum.
fn events() -> pb::Events {
    let mut odd_call = call();
    odd_call.call_type = 42.into();
    pb::Events {
        transactions: vec![
            pb::Transaction {
                hash: vec![0x11; 32],
                from: address(0xf0),
                to: Some(address(0xee)),
                nonce: 7,
                gas_price: "1000000000".into(),
                gas_limit: 300_000,
                gas_used: 150_000,
                value: "0".into(),
                logs: vec![log(Some(swap()), Some(call())), log(None, Some(call())), log(Some(swap_fee()), None)],
            },
            pb::Transaction {
                hash: vec![0x22; 32],
                from: address(0xf1),
                to: None,
                nonce: 8,
                gas_price: "2".into(),
                gas_limit: 1,
                gas_used: 1,
                value: "123".into(),
                logs: vec![log(Some(initialize()), Some(odd_call))],
            },
        ],
    }
}

/// A change as the sink reads it: fields by column name, never by position.
/// Every field is a plain SET (`update_op` 1, the only addition of the 5.0.0
/// encoding over v0.8.0's 3.0.0).
fn columns(change: &TableChange) -> BTreeMap<&str, &str> {
    change
        .fields
        .iter()
        .map(|f| {
            assert_eq!(f.update_op, UpdateOp::UPDATE_OP_SET);
            (f.name.as_str(), f.value.as_str())
        })
        .collect()
}

fn key(change: &TableChange) -> BTreeMap<&str, &str> {
    match &change.primary_key {
        Some(PrimaryKey::CompositePk(c)) => c.keys.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect(),
        other => panic!("not a composite key: {other:?}"),
    }
}

fn log_key(tx_index: &'static str, log_index: &'static str) -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("block_hash", BLOCK_HASH),
        ("block_num", "23000000"),
        ("log_index", log_index),
        ("minute", "29333334"),
        ("timestamp", "1760000042"),
        ("tx_index", tx_index),
    ])
}

#[test]
fn rows_follow_the_v0_8_0_format() {
    let changes = changes(&clock(), &events(), Encoding::Hex).unwrap().table_changes;

    let tables: Vec<&str> = changes.iter().map(|c| c.table.as_str()).collect();
    assert_eq!(tables, ["swaps", "swap_fee", "initialize", "blocks"]);
    for (i, change) in changes.iter().enumerate() {
        assert_eq!(change.ordinal, i as u64);
        assert_eq!(change.operation, Operation::OPERATION_CREATE);
        // Sorted by name, so the bytes do not depend on `HashMap` order.
        assert!(change.fields.iter().map(|f| &f.name).is_sorted());
    }

    // Every column of a swap, as v0.8.0 wrote it.
    let topics = format!("0x{},0x{}", "dd".repeat(32), "01".repeat(32));
    let (f0, ee, ca, ad, fa, x10, x05, x70, x71) = (
        hex20(0xf0),
        hex20(0xee),
        hex20(0xca),
        hex20(0xad),
        hex20(0xfa),
        hex20(0x10),
        hex20(0x05),
        hex20(0x70),
        hex20(0x71),
    );
    let tx_hash = format!("0x{}", "11".repeat(32));
    let swap_row = BTreeMap::from([
        ("block_hash", BLOCK_HASH),
        ("block_num", "23000000"),
        ("call_address", ad.as_str()),
        ("call_begin_ordinal", "10"),
        ("call_caller", ca.as_str()),
        ("call_depth", "2"),
        ("call_end_ordinal", "20"),
        ("call_gas_consumed", "21000"),
        ("call_gas_limit", "30000"),
        ("call_index", "3"),
        ("call_parent_index", "1"),
        ("call_type", "CALL_TYPE_DELEGATE"),
        ("call_value", "5"),
        ("factory", fa.as_str()),
        ("input_amount", "1000"),
        ("input_contract", x70.as_str()),
        ("log_address", x10.as_str()),
        ("log_block_index", "5"),
        ("log_data", "0x0102"),
        ("log_index", "0"),
        ("log_ordinal", "77"),
        ("log_topics", topics.as_str()),
        ("minute", "29333334"),
        ("output_amount", MAX_UINT256),
        ("output_contract", x71.as_str()),
        ("pool", x10.as_str()),
        ("protocol", "uniswap_v3"),
        ("timestamp", "1760000042"),
        ("tx_from", f0.as_str()),
        ("tx_gas_limit", "300000"),
        ("tx_gas_price", "1000000000"),
        ("tx_gas_used", "150000"),
        ("tx_hash", tx_hash.as_str()),
        ("tx_index", "0"),
        ("tx_nonce", "7"),
        ("tx_to", ee.as_str()),
        ("tx_value", "0"),
        ("user", x05.as_str()),
    ]);
    assert_eq!(columns(&changes[0]), swap_row);
    assert_eq!(key(&changes[0]), log_key("0", "0"));

    // The log without an event writes no row but keeps its index: the fee
    // setting is log 2 of transaction 0. Without call metadata, the call
    // columns take their defaults.
    let fee_row = columns(&changes[1]);
    assert_eq!(key(&changes[1]), log_key("0", "2"));
    let shared: Vec<&str> = swap_row.keys().copied().filter(|c| !c.starts_with("call_") && *c != "log_index").collect();
    let event_columns = [
        "protocol",
        "factory",
        "pool",
        "user",
        "input_contract",
        "input_amount",
        "output_contract",
        "output_amount",
    ];
    for column in shared.iter().filter(|c| !event_columns.contains(c)) {
        assert_eq!(fee_row[column], swap_row[column], "{column}");
    }
    let fb = hex20(0xfb);
    let x11 = hex20(0x11);
    let expected = [
        ("log_index", "2"),
        ("call_caller", "0x"),
        ("call_address", "0x"),
        ("call_index", "0"),
        ("call_begin_ordinal", "0"),
        ("call_end_ordinal", "0"),
        ("call_value", ""),
        ("call_gas_consumed", "0"),
        ("call_gas_limit", "0"),
        ("call_depth", "0"),
        ("call_parent_index", "0"),
        ("call_type", "CALL_TYPE_UNSPECIFIED"),
        ("protocol", "curvefi"),
        ("factory", fb.as_str()),
        ("pool", x11.as_str()),
        ("fee", "4000000"),
    ];
    for (column, value) in expected {
        assert_eq!(fee_row[column], value, "{column}");
    }
    assert_eq!(fee_row.len(), 34);

    // A contract creation has an empty `tx_to`; a call type outside the enum
    // is written as CALL_TYPE_UNSPECIFIED.
    let init_row = columns(&changes[2]);
    assert_eq!(key(&changes[2]), log_key("1", "0"));
    let fc = hex20(0xfc);
    let x12 = hex20(0x12);
    let f1 = hex20(0xf1);
    let tx_hash = format!("0x{}", "22".repeat(32));
    let expected = [
        ("tx_index", "1"),
        ("tx_hash", tx_hash.as_str()),
        ("tx_from", f1.as_str()),
        ("tx_to", ""),
        ("tx_nonce", "8"),
        ("tx_value", "123"),
        ("log_index", "0"),
        ("call_caller", ca.as_str()),
        ("call_type", "CALL_TYPE_UNSPECIFIED"),
        ("protocol", "uniswap_v2"),
        ("factory", fc.as_str()),
        ("pool", x12.as_str()),
    ];
    for (column, value) in expected {
        assert_eq!(init_row[column], value, "{column}");
    }
    let mut init_columns: BTreeSet<&str> = shared.iter().copied().filter(|c| !event_columns.contains(c)).collect();
    init_columns.extend(swap_row.keys().filter(|c| c.starts_with("call_")));
    init_columns.extend(["log_index", "protocol", "factory", "pool"]);
    assert_eq!(init_row.keys().copied().collect::<BTreeSet<_>>(), init_columns);
    assert_eq!(init_row.len(), 33);

    // One `blocks` row, keyed by `block_num`.
    let blocks = &changes[3];
    assert!(matches!(&blocks.primary_key, Some(PrimaryKey::CompositePk(c)) if c.keys.len() == 1 && c.keys["block_num"] == "23000000"));
    assert_eq!(
        columns(blocks),
        BTreeMap::from([
            ("block_hash", BLOCK_HASH),
            ("block_num", "23000000"),
            ("minute", "29333334"),
            ("timestamp", "1760000042"),
        ])
    );
}

#[test]
fn tron_base58_encodes_addresses_only() {
    // Vectors of substreams-evm `common/tests/address_tests.rs`, and USDT.
    let usdt = Hex::decode("a614f803b6fd780986a42c78ec9c7f77e6ded13c").unwrap();
    let with_version = Hex::decode("41d002c2f5b91353c1320e4d194d2d10e6b31dbe33").unwrap();
    let solscan_1 = Hex::decode("5e5602e054d1da6a43c83d9f8774daf15d79d210").unwrap();
    let solscan_2 = Hex::decode("72c28b75b5e60469ac187174a296683680ae8a58").unwrap();
    let mut events = events();
    let tx = &mut events.transactions[0];
    tx.from = usdt.clone();
    tx.to = Some(with_version);
    tx.logs[0].address = vec![0; 20];
    let call = tx.logs[0].call.as_option_mut().unwrap();
    call.caller = solscan_1;
    call.address = solscan_2;
    let Some(pb::log::Log::Swap(swap)) = &mut tx.logs[0].log else {
        unreachable!()
    };
    swap.factory = vec![0x41; 19];
    swap.pool = vec![0x10; 32];
    swap.user = vec![];
    swap.input_token = usdt;
    swap.output_token = vec![0; 20];

    let hex = changes(&clock(), &events, Encoding::Hex).unwrap().table_changes;
    let tron = changes(&clock(), &events, Encoding::TronBase58).unwrap().table_changes;
    let swap_row = columns(&tron[0]);
    for (column, value) in [
        ("tx_from", "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t"),
        ("tx_to", "TUw4rqTx1tBvj5RosPfRW2dv38HoaC7yaR"),
        ("log_address", "T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWwb"),
        ("call_caller", "TJa1ZFq2UzaXZqfw7F7hjfRmiQxWWBkCzS"),
        ("call_address", "TLS168n4rPdVbZQDoDCjj57axRZTyZ9vJh"),
        // 19 and 32 bytes, and none: empty strings.
        ("factory", ""),
        ("pool", ""),
        ("user", ""),
        ("input_contract", "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t"),
        ("output_contract", "T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWwb"),
    ] {
        assert_eq!(swap_row[column], value, "{column}");
    }
    let hex_row = columns(&hex[0]);
    assert_eq!((hex_row["factory"], hex_row["user"]), (format!("0x{}", "41".repeat(19)).as_str(), "0x"));
    // Without call metadata: empty in Tron Base58, `0x` in hex.
    let fee_row = columns(&tron[1]);
    assert_eq!((fee_row["call_caller"], fee_row["call_address"]), ("", ""));

    // Hashes, topics, data and every other column are the same in both.
    assert_eq!(hex.len(), tron.len());
    for (hex, tron) in hex.iter().zip(&tron) {
        assert_eq!((&hex.table, key(hex)), (&tron.table, key(tron)));
        let (hex, tron) = (columns(hex), columns(tron));
        assert_eq!(hex.keys().collect::<Vec<_>>(), tron.keys().collect::<Vec<_>>());
        for (column, value) in &hex {
            if !ADDRESS_COLUMNS.contains(column) {
                assert_eq!(tron[column], *value, "{column}");
            }
        }
    }
    assert!(swap_row["tx_hash"].starts_with("0x") && swap_row["log_topics"].starts_with("0x") && swap_row["log_data"] == "0x0102");
}

#[test]
fn a_block_without_events_writes_nothing() {
    let mut no_time = clock();
    no_time.timestamp = buffa::MessageField::none();
    let only_logs_without_events = pb::Events {
        transactions: vec![pb::Transaction {
            logs: vec![log(None, Some(call())), log(None, None)],
            ..Default::default()
        }],
    };
    let no_logs = pb::Events {
        transactions: vec![pb::Transaction::default()],
    };
    for events in [pb::Events::default(), no_logs, only_logs_without_events] {
        for clock in [clock(), no_time.clone()] {
            // v0.8.0 read the timestamp only to write a row.
            for encoding in [Encoding::Hex, Encoding::TronBase58] {
                assert!(changes(&clock, &events, encoding).unwrap().table_changes.is_empty());
            }
        }
    }
}

/// Where v0.8.0 panicked, the port fails the block with an error.
#[test]
fn a_block_with_events_and_a_clock_without_timestamp_fails() {
    let mut no_time = clock();
    no_time.timestamp = buffa::MessageField::none();
    let err = changes(&no_time, &events(), Encoding::Hex).unwrap_err();
    assert!(err.to_string().contains("timestamp"), "{err}");
}

/// The `protocol` column is the same `Enum8` in the nine tables that have
/// one; every protocol of `dex.swaps.v1` writes one of its names.
#[test]
fn protocols_write_the_names_of_the_clickhouse_enum() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("clickhouse");
    let mut enums = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if !name.starts_with("schema.") || !name.ends_with(".sql") || name == "schema.sql" {
            continue;
        }
        let sql = std::fs::read_to_string(dir.join(name)).unwrap();
        for definition in sql.split("Enum8(").skip(1) {
            let body = &definition[..definition.find(')').unwrap()];
            let values: Vec<(String, i8)> = body
                .split(',')
                .map(|entry| {
                    let (name, value) = entry.split_once('=').unwrap();
                    (name.trim().trim_matches('\'').to_string(), value.trim().parse().unwrap())
                })
                .collect();
            enums.push(values);
        }
    }
    assert_eq!(enums.len(), 9, "swaps, swap_fee, initialize and six state tables");
    assert!(enums.iter().all(|e| *e == enums[0]));
    let clickhouse: BTreeSet<&str> = enums[0].iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(clickhouse.len(), 14);

    let protocol_of = |protocol: buffa::EnumValue<pb::Protocol>| {
        let mut events = events();
        events.transactions.truncate(1);
        let Some(pb::log::Log::Swap(swap)) = &mut events.transactions[0].logs[0].log else {
            unreachable!()
        };
        swap.protocol = protocol;
        let changes = changes(&clock(), &events, Encoding::Hex).unwrap();
        columns(&changes.table_changes[0])["protocol"].to_string()
    };
    let written: BTreeSet<String> = (1..=14).map(|p| protocol_of(p.into())).collect();
    assert_eq!(written.iter().map(String::as_str).collect::<BTreeSet<_>>(), clickhouse);
    // Kept from v0.8.0: no Enum8 value, so ClickHouse rejects the row.
    // `dex-swaps` always sets a protocol.
    for unknown in [0, 15, -1] {
        assert_eq!(protocol_of(unknown.into()), "unspecified");
    }
}

/// The handler takes `(params, Clock, dex_swaps:map_events)`: the manifest
/// must feed the inputs in that order, default the param to `hex` and embed
/// the generated schema.
#[test]
fn manifest_wires_db_out_and_the_schema() {
    assert!(MANIFEST.contains(
        "    inputs:\n      - params: string\n      - source: sf.substreams.v1.Clock\n      - map: dex_swaps:map_events\n    output:\n      type: proto:sf.substreams.sink.database.v1.DatabaseChanges\n"
    ));
    assert!(MANIFEST.contains("\n  dex_swaps: ../dex/swaps/substreams.yaml\n"));
    assert!(MANIFEST.contains("\nparams:\n  db_out: \"hex\""));
    assert!(MANIFEST.contains("sink:\n  module: db_out\n  type: sf.substreams.sink.sql.v1.Service\n  config:\n    schema: \"./clickhouse/schema.sql\""));
    assert!(MANIFEST.contains("    engine: clickhouse\n"));
    assert!(MANIFEST.contains("\nnetwork: mainnet\n"));
    assert_eq!(Encoding::from_param("hex").unwrap(), Encoding::Hex);
}

/// The SQL files are v0.8.0's, byte for byte, and `make schema` turns them
/// into the schema embedded in substreams-evm's `evm-clickhouse-dex-v0.8.0`.
#[test]
fn schema_is_the_v0_8_0_schema() {
    const FILES: [(&str, &str); 13] = [
        ("schema.0.blocks.sql", "1156819b835563c8a87ec43f8d674092f04f4eb71e501189e0d72e89527f8af3"),
        ("schema.0.templates.sql", "ec4a1a24b6137b0869c07dfd463fed5ed3ed393262d8ed2550ce834e59402b67"),
        (
            "schema.1.table.initialize.sql",
            "7aa1363ee3e2829a38822241f9f19614f292b4351c75cbb377bd04aceeb986aa",
        ),
        (
            "schema.1.table.swap_fee.sql",
            "5ebda37efbd5489d89a0aaf5515026853d8caad5ec528ee9f495ea1fb31c3e65",
        ),
        ("schema.1.table.swaps.sql", "504196005dc3befdc7470e9192caee365747ae1c8e28f136bf8d6cfa8ce5683c"),
        (
            "schema.2.mv.state_pools_fees.sql",
            "76c5331b12d49fdc2f3acdcfa98b7cb67777bd82c77b29725f5d3b44ea8a00ca",
        ),
        (
            "schema.2.mv.state_pools_initialize.sql",
            "c4296bfe87fb708e550b91d3b8d56220acd8af295f22364812f4732e5d8ea010",
        ),
        (
            "schema.3.mv.state_ohlc_prices.sql",
            "4ba455c49d2e7a2fd9267b2228cca954923e0d73df669e2696ca2c57340b45c0",
        ),
        (
            "schema.3.mv.state_pools_aggregating_by_pool.sql",
            "05462c3eab5aba3520f22c9a2dd6bab9d681e280bd27fb1e95aa32dd50f0905d",
        ),
        (
            "schema.3.mv.state_pools_aggregating_by_token.sql",
            "dd9d2725fdfb1eab9e4abb8d31aaa08bd455f48c4b56095f7d3e43b54f13f057",
        ),
        (
            "schema.3.mv.state_pools_uaw.sql",
            "c44c48037d6ca7c59b0f754314bd8b6b5262f3d7932dc5a93ea097682ae0c19e",
        ),
        (
            "schema.4.view.ohlc_prices.sql",
            "6068e371815e217abb0f5e5fd55f2969891a352739e58907553b6373e04473c4",
        ),
        ("schema.4.view.pools.sql", "4c198dd94647c5319457d544349a3ae539862489eb45fc36a54d1a00ef52cd9b"),
    ];
    let hex = |bytes: &[u8]| Hex::encode(Sha256::digest(bytes));
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("clickhouse");

    let mut parts: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("schema.") && n.ends_with(".sql") && n != "schema.sql")
        .collect();
    parts.sort();
    assert_eq!(parts, FILES.map(|(name, _)| name));

    let mut schema = b"-- This file is generated. Do not edit.\n\n".to_vec();
    for (name, sha256) in FILES {
        let sql = std::fs::read(dir.join(name)).unwrap();
        assert_eq!(hex(&sql), sha256, "{name}");
        schema.extend_from_slice(&sql);
        schema.push(b'\n');
    }
    assert_eq!(schema.len(), 32_386);
    assert_eq!(hex(&schema), "93d2277dba6f50472a9f58cf8b0baf71d65df70b724b338d8177909ac060543d");
}

/// The manifest compiles `DatabaseChanges` from the source that
/// substreams-database-change 5.0.0 ships, so the package describes the
/// encoding `db_out` emits (`value`, `update_op`).
#[test]
fn database_change_descriptor_is_the_5_0_0_source() {
    let proto = include_bytes!("../proto/sf/substreams/sink/database/v1/database.proto");
    assert_eq!(
        Hex::encode(Sha256::digest(proto)),
        "122db4ce295c1e4e24d66a55d5796cdb950599ffeeb820684d61f630721094f7"
    );
    assert!(MANIFEST.contains("protobuf:\n  files:\n    - sf/substreams/sink/database/v1/database.proto\n  importPaths:\n    - ./proto\n"));
}
