# evm-dex

One `db_out` writes the swaps, swap fees and pool initializations of
[`dex/swaps`](../dex/swaps/README.md) into the Token API DEX ClickHouse
tables. A port of substreams-evm `evm-dex` at tag `evm-dex-v0.8.0`
(`9663b25`), the unified-swaps design the owner chose on 2026-10-13, to
substreams 0.8.0, buffa and `substreams-database-change` 5.0.0. The rows are
v0.8.0's, including the behaviour listed under
[Kept from v0.8.0](#kept-from-v080). The SQL is v0.8.0's, byte for byte, and
the package embeds it, so `substreams-sink-sql setup` needs only the package.

v0.8.0 was never deployed: before the Token API switches, check these tables
against the production v0.5.0 tables (a live step for the owner).

```
params (hex | tron_base58) ─┐
Clock ──────────────────────┼─▶ db_out ─▶ DatabaseChanges ─▶ substreams-sink-sql
dex_swaps:map_events ───────┘

swaps ──▶ state_ohlc_prices ──▶ ohlc_prices, ohlc_prices_uaw (views)
      ──▶ state_pools_aggregating_by_pool ──▶ pools_by_pool (view)
      ──▶ state_pools_aggregating_by_token (input and output token) ──▶ pools_by_token (view)
      ──▶ state_pools_uaw (user, tx_from and call_caller)
swap_fee ──▶ state_pools_fees
initialize ──▶ state_pools_initialize
blocks
```

Each arrow out of a table is a materialized view named after its target
(`mv_state_…`); `state_pools_aggregating_by_token` has one per token side.

## Rows

| Table | Key | Columns | One row per |
| --- | --- | --- | --- |
| `swaps` | `(minute, timestamp, block_num, tx_index, log_index, block_hash)` | the 30 shared columns, then `protocol`, `factory`, `pool`, `user`, `input_contract`, `input_amount`, `output_contract`, `output_amount` | `Swap` |
| `swap_fee` | same | the 30 shared columns, then `protocol`, `factory`, `pool`, `fee` | `SwapFee` |
| `initialize` | same | the 30 shared columns, then `protocol`, `factory`, `pool` | `Initialize` |
| `blocks` | `block_num` | `block_num`, `block_hash`, `timestamp`, `minute` | block that wrote any other row |

The 30 shared columns are those of the SQL `TEMPLATE_LOG`, without the
`MATERIALIZED` topics:

| Columns | Value |
| --- | --- |
| `block_num`, `block_hash`, `timestamp`, `minute` | the clock: number, `0x` + id, seconds, seconds / 60 |
| `tx_index` | position of the transaction in `Events.transactions` (transactions with events only) |
| `tx_hash` | hex |
| `tx_from`, `tx_to` | addresses; `tx_to` is empty for a contract creation |
| `tx_nonce`, `tx_gas_price`, `tx_gas_limit`, `tx_gas_used`, `tx_value` | verbatim |
| `log_index` | position of the log in `Transaction.logs`, counting logs without an event |
| `log_block_index`, `log_ordinal` | verbatim |
| `log_address` | address |
| `log_topics`, `log_data` | hex; the topics comma-separated |
| `call_caller`, `call_address` | addresses |
| `call_index`, `call_begin_ordinal`, `call_end_ordinal`, `call_value`, `call_gas_consumed`, `call_gas_limit`, `call_depth`, `call_parent_index` | verbatim |
| `call_type` | the proto name, such as `CALL_TYPE_CALL` |

- **Event columns.** `protocol` is the lowercase protocol name, such as
  `uniswap_v3`, one of the names of the tables' `Enum8`. `factory`, `pool`,
  `user`, `input_contract` (`input_token`) and `output_contract`
  (`output_token`) are addresses; `input_amount`, `output_amount` and `fee`
  are verbatim.
- **Addresses.** The `db_out` param selects the encoding, `hex` by default:
  `0x` + lowercase hex, or with `tron_base58` Tron Base58Check (`T...`). In
  Base58, an address of 20 bytes gets the `0x41` version byte, one of 21
  bytes must start with it, and any other length, none included, is written
  as an empty string. Any other param fails the block. Hashes, topics and data
  are hex in both encodings.
- **Operation and order.** Every row is a CREATE, which ClickHouse inserts, in
  event order, then the `blocks` row. A block without events writes nothing.
  ClickHouse materializes `log_topic0`–`log_topic3` and the swaps' canonical
  `token0`, `token1`, `amount0` and `amount1`.
- **Fields.** Sorted by name: `substreams-database-change` keeps a row in a
  `HashMap`, in 5.0.0 as in 3.0.0, and the sink matches columns by name. The
  composite keys stay in map order.
- **Insert-only.** The log tables and `blocks` are plain `MergeTree`s, so a
  replayed block inserts its rows again. No table stores a chain id: use one
  database per network.

## Host API

`evm_dex::changes(&Clock, &pb::Events, Encoding) -> Result<DatabaseChanges, Error>`
is the `db_out` body without the Substreams runtime, for a host program that
runs `dex_swaps::project` itself. `Encoding::from_param(&str)` parses the
param; `pb` re-exports `dex.swaps.v1`. It fails, where v0.8.0 panicked, on a
clock without a timestamp in a block with events; a block without events
writes nothing whatever its clock.

Native consumers accepting bounded inputs should use
`changes_bounded(&Clock, &pb::Events, Encoding, max_logs, max_bytes)`. Limits
cover all input logs (including entries without an event), encoded input plus
clock, and exact encoded output. Each candidate row is checked before it joins
the retained batch, so repeated transaction or clock fields cannot expand across
the whole block before rejection. At most one input-bounded candidate row sits
outside the output budget. Invalid limits or overflow return an error without
partial output. Row contents, keys, operations and ordinals match `changes`.

## Kept from v0.8.0

The port writes v0.8.0's rows exactly, including behaviour that looks wrong.
Changing any of it is a separate change the owner approves.

1. **Positions, not block indexes.** `tx_index` and `log_index` are positions
   in `dex_swaps:map_events`' output, and they key the rows. A log that two
   protocols decode (Uniswap V3 and Kyber Elastic share `Swap`, see
   [`dex/swaps`](../dex/swaps/README.md#kept-from-v080)) gives two rows with
   consecutive `log_index` and one `log_block_index`.
2. **`unspecified` protocol.** `PROTOCOL_UNSPECIFIED` or a value outside the
   enum writes `unspecified`, which no `Enum8` lists: ClickHouse rejects the
   row (`UNKNOWN_ELEMENT_OF_ENUM`). `dex-swaps` always sets a protocol.
3. **Logs without call metadata.** `dex-swaps` takes a transaction's logs from
   its receipt when the transaction has no call traces, as in blocks without
   them. Their call columns are then
   the defaults: `call_caller` and `call_address` `0x` in hex (empty in
   Base58), zeros, an empty `call_value` in a `UInt256` column and
   `CALL_TYPE_UNSPECIFIED`. `mv_state_pools_uaw` skips only empty addresses,
   so in hex it counts `0x` as one more unique wallet of each such pool. A
   call type outside the enum is also written `CALL_TYPE_UNSPECIFIED`.
4. **Base58 lengths.** With `tron_base58`, an address that is not 20 bytes (or
   21 with the version byte) is written empty, such as a Uniswap V4 pool id
   (32 bytes).

## Port from v0.8.0

Every file comes from substreams-evm `9663b25` (tag `evm-dex-v0.8.0`) unless
noted:

| File | Source | Changes |
| --- | --- | --- |
| `src/lib.rs` | `evm-dex/src/lib.rs` | `db_out` in a wasm32-only module over `changes`; errors instead of panics; fields sorted; `substreams-database-change` 5.0.0 |
| `src/columns.rs` | `common/src/clickhouse.rs` | `log_key`, `set_clock` and the transaction, log and call templates for `dex.swaps.v1` only, without the traits |
| `src/encoding.rs` | `common/src/lib.rs` | `Encoding`, `handle_encoding_param` (as `Encoding::from_param`, an error instead of a panic), `bytes_to_hex`, `bytes_to_string`, `tron_base58_from_bytes` |
| `clickhouse/schema.*.sql` (13 files) | `evm-dex/clickhouse/` | none (byte-identical; sha256 values in [`tests/db_out.rs`](tests/db_out.rs)) |
| `clickhouse/schema.sql` | the schema embedded in `spkg/evm-clickhouse-dex-v0.8.0.spkg` | generated by `make schema`, not committed: 32,386 bytes, sha256 `93d2277d…`, MD5 `8e3e3dfc…`, the same bytes |
| `clickhouse/examples/{OHLCV,Swaps}.sql` | `evm-dex/clickhouse/examples/` | none (byte-identical) |
| `substreams.yaml` | `evm-dex/substreams.yaml` and `evm-dex/clickhouse/substreams.yaml` | one manifest, `evm_dex` v0.9.0 with the `sink:` section; imports `../dex/swaps/substreams.yaml` instead of the v0.8.0 spkg; same module, inputs and `hex` default |
| `Makefile` | `evm-dex/Makefile` and `evm-dex/clickhouse/Makefile` | `build`, `schema` (the same recipe, in byte order) and `pack` only |
| `proto/sf/substreams/sink/database/v1/database.proto` | `substreams-database-change` 5.0.0 ([`dae3f279`](https://github.com/streamingfast/substreams-sink-database-changes/blob/dae3f279f3d93518ac0f81e1ef71b82b4f7ec1b7/proto/sf/substreams/sink/database/v1/database.proto)) | none (byte-identical, sha256 `122db4ce…`) |

The Base58 test vectors come from `common/tests/address_tests.rs`.

Deviations from v0.8.0, none of which changes a stored row:
- **Errors instead of panics** on a param other than `hex`, `tron_base58` or
  empty, and on a clock without a timestamp in a block with events. The block
  fails either way.
- **`substreams-database-change` 5.0.0** (buffa, substreams 0.8.0) in place of
  3.0.0. On the wire, field 2 of `Field` keeps its number (`new_value` in
  3.0.0, `value` in 5.0.0), field 3 (`old_value`) was always empty, and every
  field now also carries `update_op = UPDATE_OP_SET` (field 4). The SQL sink
  uses only the name and value of a CREATE's fields. Fields are sorted by name.
- **Descriptors.** The manifest compiles `DatabaseChanges` from the 5.0.0
  source above, so the package describes `value` and `update_op`; the CLI adds
  `sf.substreams.sink.sql.v1.Service` from its built-in
  `sf/substreams/sink/sql/v1/deprecated.proto` (it refuses that path in
  `protobuf.files`). v0.8.0 imported `substreams-database-change-v2.0.0.spkg`
  and `substreams-sink-sql-protodefs-v1.0.7.spkg` instead.

Not ported: `clickhouse/examples/UAW-benchmarks.sql`, which queries
`state_pools_uaw_by_{tx_from,user,call_caller}`, tables the v0.8.0 schema does
not define; `clickhouse/scripts/` (an RPC and ClickHouse comparison script and
its `.env.example`); `pricing.md`; the wrapper manifest and the image; the
Makefiles' live targets.

## Validation

`cargo test -p evm-dex` pins the 13 SQL files, the generated schema and the
vendored proto by sha256, checks every column of each table for both
encodings, the errors, the manifest wiring and that the protocol names are
those of the nine identical `Enum8`s. The pull request that added this package
reports the offline checks: a differential run against v0.8.0's `db_out` on
random synthetic events, the schema and rows in `clickhouse-local`, and the
module hashes of a local pack. No live run backs this package.

## Build

From the repository root, with the pinned toolchain:

```sh
make -C evm-dex build    # dex/pools, dex/swaps and evm-dex WASM, one cargo build each
make -C evm-dex schema   # clickhouse/schema.sql
make -C evm-dex pack     # out/evm-dex-v0.9.0.spkg
```

`substreams pack` embeds the imported packages' WASM without building it, and
it reads `clickhouse/schema.sql`, so `pack` runs both. The three WASMs are
built one package at a time: built together, `substreams-database-change`
turns on buffa's `json` feature for the imported maps too, and their WASM and
module hashes would differ from the `dex/pools` and `dex/swaps` packages'.
Committed spkgs come from CI's Linux build, never a local pack.

The Token API runs one database per chain, `<chain>:evm-dex@<version>`, from
the release spkg. Tron needs the `db_out` param `tron_base58`.


## Immutable released-WASM comparison

The [v0.8.0 oracle corpus](../dex/swaps/tests/fixtures/legacy-v0.8.0/README.md)
executes the actual released pool store, swap map and database formatter. Its 73
synthetic cases cover every supported swap protocol, metadata boundaries,
first-writer behavior and both address encodings. Seventy-one cases agree in
state/events/row fields and order; two reject in both implementations (old WASM
panic, native error). It retains every known v0.8.0 behavior. The frozen corpus
and binary hashes are checked offline with `cargo test --locked -p dex-swaps
released_v080`; Wasmi is a dev dependency only. This does not establish live
or captured-block coverage or parity with the Token API's older v0.5.0 sinks.
