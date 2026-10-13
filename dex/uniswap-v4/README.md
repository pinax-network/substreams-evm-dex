# Uniswap V4 events

Uniswap V4 PoolManager events; pools are identified by their 32-byte id. A port of substreams-evm `dex/uniswap-v4` at tag `evm-dex-v0.8.0` (`9663b25`) to substreams 0.8.0, buffa and substreams-abis v2.0.0. Its output is unchanged, including the v0.8.0 behaviour listed under [Kept from v0.8.0](#kept-from-v080).

`map_events(Block)` emits `uniswap.v4.Events`: per successful transaction with at least one decoded log, the transaction, then each decoded log with its raw fields (address, ordinal, topics, data, block index), its emitting call (when the block has call traces; otherwise the receipt's logs, with no call) and the decoded event. Every emitter counts: nothing checks that a log comes from a known contract.

## Events

| Event | `substreams-abis` module | `Log.log` |
| --- | --- | --- |
| `Swap(bytes32,address,int128,int128,uint160,uint128,int24,uint24)` | `uniswap::v4::poolmanager` | `Swap` |
| `Initialize(bytes32,address,address,uint24,int24,address,uint160,int24)` | `uniswap::v4::poolmanager` | `Initialize` |
| `ModifyLiquidity(bytes32,address,int24,int24,int256,bytes32)` | `uniswap::v4::poolmanager` | `ModifyLiquidity` |
| `Donate(bytes32,address,uint256,uint256)` | `uniswap::v4::poolmanager` | `Donate` |
| `ProtocolFeeControllerUpdated(address)` | `uniswap::v4::poolmanager` | `ProtocolFeeControllerUpdated` |
| `ProtocolFeeUpdated(bytes32,uint24)` | `uniswap::v4::poolmanager` | `ProtocolFeeUpdated` |

Amounts are exact decimal strings; addresses and hashes are raw bytes.

## Host API

`uniswap_v4::project(&eth::Block) -> Result<pb::Events, Error>` runs the same decoding without the Substreams runtime, for a host program such as `firehose-parquet` that decodes blocks itself. `eth` and `pb` re-export the input and output types. The `map_events` handler is compiled only for wasm32.

`project` fails the block where v0.8.0 panicked: a successful transaction without a receipt.

## Kept from v0.8.0

The port reproduces v0.8.0's output exactly, including behaviour that looks wrong; changing it is a separate, owner-approved change.

1. **Negative `int24` values become 0.** v0.8.0's `common::bigint_to_i32` rejected negative values and callers used `unwrap_or_default()`, so every negative tick (`Swap`, `Initialize`), `tick_spacing` (`Initialize`) and `tick_lower`/`tick_upper` (`ModifyLiquidity`) is emitted as `0`. substreams-evm fixed the helper after v0.8.0 (`b596c2d`, on `main`); the port keeps v0.8.0's output until the owner approves the change (`common::bigint_to_i32`).
2. **`Transaction.gas_used`** is the receipt's cumulative gas, not the transaction's own.
3. **`StorePool`** is declared in the proto but never emitted: v0.8.0 has no store module.

## Port from v0.8.0

Every source file comes from substreams-evm `9663b25` (tag `evm-dex-v0.8.0`):

| File | Source | Changes |
| --- | --- | --- |
| `src/lib.rs` | `dex/uniswap-v4/src/lib.rs` | `map_events` body as `project(&Block)`, with `map_events` in a wasm32-only module; receipt check up front; buffa: oneof variants built with `Log::from` |
| `src/tests.rs` | new | one synthetic log per event type, plus the shared transaction tests |
| `substreams.yaml` | `dex/uniswap-v4/substreams.yaml` | version v0.9.0, this repository's URL, no image; same module, input and output |
| `Makefile` | `dex/uniswap-v4/Makefile` | `build` and `pack` only, with `build.mk`'s path remaps; packs to `out/` |
| `Cargo.toml` | `dex/uniswap-v4/Cargo.toml` | version 0.9.0; also an `rlib`; `common`'s `testing` feature for tests |
| `../../proto/v1/dex/uniswap-v4.proto` | `proto/v1/dex/uniswap-v4.proto` | none (byte-identical) |
| `../../common/src/create.rs` | `common/src/create.rs` | one macro implements the templates for `dex.swaps.v1` and every per-protocol package; field values unchanged |
| `../../common/src/lib.rs` | `common/src/lib.rs` | v0.8.0's `bigint_to_u64` and `bigint_to_i32`; new `try_to_u64` |

Differences from v0.8.0:

- Where v0.8.0 panicked, `project` returns an error; the block fails either way.
- substreams-abis v2.0.0's readers replace v1.5.0's `ethabi` decoding and keep its accept/reject rules; every event above has the same topic 0, fields and `match_log` rules (topic count, data length) in both versions.

## Validation

`cargo test -p uniswap-v4` runs 8 tests: one synthetic log per event type, with topic 0 the Keccak-256 of the canonical signature from the substreams-abis ABI JSON, asserting every decoded field, plus `common::transaction_tests!` (transaction, log and call metadata, the receipt fallback, reverted calls, failed transactions and a missing receipt). A differential run on 2026-10-13 fed 120,000 random synthetic blocks to this crate and to v0.8.0's (prost, substreams 0.7.6, substreams-abis v1.5.0): byte-identical `map_events` output and the same failing blocks (see the pull request). No captured-block fixture or live run backs this package yet.

## Build

From the repository root, with the pinned toolchain:

```sh
make -C dex/uniswap-v4 build   # WASM, with the build.mk path remaps
make -C dex/uniswap-v4 pack    # out/uniswap-v4-v0.9.0.spkg
```

Committed spkgs come from CI's Linux build, never a local pack.
