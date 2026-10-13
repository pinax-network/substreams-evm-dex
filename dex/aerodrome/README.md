# Aerodrome events

Aerodrome/Velodrome (Solidly fork) pool and pool factory events. A port of substreams-evm `dex/aerodrome` at tag `evm-dex-v0.8.0` (`9663b25`) to substreams 0.8.0, buffa and substreams-abis v2.0.0. Its output is unchanged, including the v0.8.0 behaviour listed under [Kept from v0.8.0](#kept-from-v080).

`map_events(Block)` emits `aerodrome.v1.Events`: per successful transaction with at least one decoded log, the transaction, then each decoded log with its raw fields (address, ordinal, topics, data, block index), its emitting call (when the block has call traces; otherwise the receipt's logs, with no call) and the decoded event. Every emitter counts: nothing checks that a log comes from a known contract.

## Events

| Event | `substreams-abis` module | `Log.log` |
| --- | --- | --- |
| `Swap(address,address,uint256,uint256,uint256,uint256)` | `aerodrome::pool` | `Swap` |
| `Sync(uint256,uint256)` | `aerodrome::pool` | `Sync` |
| `Mint(address,uint256,uint256)` | `aerodrome::pool` | `Mint` |
| `Burn(address,address,uint256,uint256)` | `aerodrome::pool` | `Burn` |
| `Fees(address,uint256,uint256)` | `aerodrome::pool` | `Fees` |
| `Claim(address,address,uint256,uint256)` | `aerodrome::pool` | `Claim` |
| `PoolCreated(address,address,bool,address,uint256)` | `aerodrome::poolfactory` | `PoolCreated` (the unnamed `uint256` as `extra_data`) |

Amounts are exact decimal strings; addresses and hashes are raw bytes.

## Host API

`aerodrome::project(&eth::Block) -> Result<pb::Events, Error>` runs the same decoding without the Substreams runtime, for a host program such as `firehose-parquet` that decodes blocks itself. `eth` and `pb` re-export the input and output types. The `map_events` handler is compiled only for wasm32.

`project` fails the block where v0.8.0 panicked: a successful transaction without a receipt.

## Kept from v0.8.0

The port reproduces v0.8.0's output exactly, including behaviour that looks wrong; changing it is a separate, owner-approved change.

1. **Shared signatures.** Uniswap V2's pair `Mint(address,uint256,uint256)` has Aerodrome's signature, so it is emitted here too.
2. **`Transaction.gas_used`** is the receipt's cumulative gas, not the transaction's own.
3. **`StorePool`** is declared in the proto but never emitted: v0.8.0 has no store module.

## Port from v0.8.0

Every source file comes from substreams-evm `9663b25` (tag `evm-dex-v0.8.0`):

| File | Source | Changes |
| --- | --- | --- |
| `src/lib.rs` | `dex/aerodrome/src/lib.rs` | `map_events` body as `project(&Block)`, with `map_events` in a wasm32-only module; receipt check up front; buffa: oneof variants built with `Log::from` |
| `src/tests.rs` | new | one synthetic log per event type, plus the shared transaction tests |
| `substreams.yaml` | `dex/aerodrome/substreams.yaml` | version v0.9.0, this repository's URL, no image; same module, input and output |
| `Makefile` | `dex/aerodrome/Makefile` | `build` and `pack` only, with `build.mk`'s path remaps; packs to `out/` |
| `Cargo.toml` | `dex/aerodrome/Cargo.toml` | version 0.9.0; also an `rlib`; `common`'s `testing` feature for tests |
| `../../proto/v1/dex/aerodrome.proto` | `proto/v1/dex/aerodrome.proto` | none (byte-identical) |
| `../../common/src/create.rs` | `common/src/create.rs` | one macro implements the templates for `dex.swaps.v1` and every per-protocol package; field values unchanged |

Differences from v0.8.0:

- Where v0.8.0 panicked, `project` returns an error; the block fails either way.
- substreams-abis v2.0.0's readers replace v1.5.0's `ethabi` decoding and keep its accept/reject rules; every event above has the same topic 0, fields and `match_log` rules (topic count, data length) in both versions.

## Validation

`cargo test -p aerodrome` runs 9 tests: one synthetic log per event type, with topic 0 the Keccak-256 of the canonical signature from the substreams-abis ABI JSON, asserting every decoded field, plus `common::transaction_tests!` (transaction, log and call metadata, the receipt fallback, reverted calls, failed transactions and a missing receipt). A differential run on 2026-10-13 fed 120,000 random synthetic blocks to this crate and to v0.8.0's (prost, substreams 0.7.6, substreams-abis v1.5.0): byte-identical `map_events` output and the same failing blocks (see the pull request). No captured-block fixture or live run backs this package yet.

## Build

From the repository root, with the pinned toolchain:

```sh
make -C dex/aerodrome build   # WASM, with the build.mk path remaps
make -C dex/aerodrome pack    # out/aerodrome-v0.9.0.spkg
```

Committed spkgs come from CI's Linux build, never a local pack.
