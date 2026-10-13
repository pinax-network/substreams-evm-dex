# Trader Joe V2 (Liquidity Book) events

Trader Joe V2 Liquidity Book pair and factory events. Each packed `bytes32` amount is split into two `uint128` decimal strings, `*_x` from the high and `*_y` from the low 128 bits; `DepositedToBins` and `WithdrawnFromBins` keep the packed words. A port of substreams-evm `dex/traderjoe` at tag `evm-dex-v0.8.0` (`9663b25`) to substreams 0.8.0, buffa and substreams-abis v2.0.0. Its output is unchanged, including the v0.8.0 behaviour listed under [Kept from v0.8.0](#kept-from-v080).

`map_events(Block)` emits `traderjoe.v1.Events`: per successful transaction with at least one decoded log, the transaction, then each decoded log with its raw fields (address, ordinal, topics, data, block index), its emitting call (when the block has call traces; otherwise the receipt's logs, with no call) and the decoded event. Every emitter counts: nothing checks that a log comes from a known contract.

## Events

| Event | `substreams-abis` module | `Log.log` |
| --- | --- | --- |
| `Swap(address,address,uint24,bytes32,bytes32,uint24,bytes32,bytes32)` | `traderjoe::lbpair` | `Swap` |
| `DepositedToBins(address,address,uint256[],bytes32[])` | `traderjoe::lbpair` | `DepositedToBins` |
| `WithdrawnFromBins(address,address,uint256[],bytes32[])` | `traderjoe::lbpair` | `WithdrawnFromBins` |
| `CompositionFees(address,uint24,bytes32,bytes32)` | `traderjoe::lbpair` | `CompositionFees` |
| `LBPairCreated(address,address,uint256,address,uint256)` | `traderjoe::lbfactory` | `LbPairCreated` |

Amounts are exact decimal strings; addresses and hashes are raw bytes.

## Host API

`traderjoe::project(&eth::Block) -> Result<pb::Events, Error>` runs the same decoding without the Substreams runtime, for a host program such as `firehose-parquet` that decodes blocks itself. `eth` and `pb` re-export the input and output types. The `map_events` handler is compiled only for wasm32.

`project` fails the block where v0.8.0 panicked: a successful transaction without a receipt; or a `Swap` or `CompositionFees` bin id, a `Swap` volatility accumulator, a `DepositedToBins` or `WithdrawnFromBins` id, or an `LBPairCreated` bin step or pid above `u64::MAX` (`to_u64()`).

## Kept from v0.8.0

The port reproduces v0.8.0's output exactly, including behaviour that looks wrong; changing it is a separate, owner-approved change.

1. **X and Y are probably swapped.** v0.8.0 reads the high 128 bits of a packed amount as X. Liquidity Book packs X in the low 128 bits (`dex/swaps` README, item 3), so `amount_in_x` probably holds the Y amount and the reverse.
2. **Narrow fields wrap.** `id`, `volatility_accumulator`, `bin_step` and `pid` are `to_u64() as u32`: a value above `u32::MAX` keeps its low 32 bits (a malformed log only).
3. **`Transaction.gas_used`** is the receipt's cumulative gas, not the transaction's own.
4. **`StorePool`** is declared in the proto but never emitted: v0.8.0 has no store module.

## Port from v0.8.0

Every source file comes from substreams-evm `9663b25` (tag `evm-dex-v0.8.0`):

| File | Source | Changes |
| --- | --- | --- |
| `src/lib.rs` | `dex/traderjoe/src/lib.rs` | `map_events` body as `project(&Block)`, with `map_events` in a wasm32-only module; receipt check up front; buffa: oneof variants built with `Log::from`; `to_u64()` through `common::try_to_u64` (an error, not a panic) |
| `src/tests.rs` | new | one synthetic log per event type, plus the shared transaction tests |
| `substreams.yaml` | `dex/traderjoe/substreams.yaml` | version v0.9.0, this repository's URL, no image; same module, input and output |
| `Makefile` | `dex/traderjoe/Makefile` | `build` and `pack` only, with `build.mk`'s path remaps; packs to `out/` |
| `Cargo.toml` | `dex/traderjoe/Cargo.toml` | version 0.9.0; also an `rlib`; `common`'s `testing` feature for tests |
| `../../proto/v1/dex/traderjoe.proto` | `proto/v1/dex/traderjoe.proto` | none (byte-identical) |
| `../../common/src/create.rs` | `common/src/create.rs` | one macro implements the templates for `dex.swaps.v1` and every per-protocol package; field values unchanged |
| `../../common/src/lib.rs` | `common/src/lib.rs` | v0.8.0's `bigint_to_u64` and `bigint_to_i32`; new `try_to_u64` |

v0.8.0's `dex/traderjoe/README.md` documented the upper-is-X layout this package keeps; this README replaces it.

Differences from v0.8.0:

- Where v0.8.0 panicked, `project` returns an error; the block fails either way.
- substreams-abis v2.0.0's readers replace v1.5.0's `ethabi` decoding and keep its accept/reject rules; every event above has the same topic 0, fields and `match_log` rules (topic count, data length) in both versions.
- On wasm32 only (by inspection of `ethabi` 17.2's decoder, not exercised): v1.5.0's `ethabi` adds offsets and lengths read from the log in a 32-bit `usize` without a check, so a `DepositedToBins` or `WithdrawnFromBins` log whose offset or length makes that sum wrap (an offset within 32 of `u32::MAX`, for one) made v0.8.0 panic, failing the block; v2.0.0's readers reject the log, so the port skips it. On a 64-bit host both reject it, which is what the differential run below compares.

## Validation

`cargo test -p traderjoe` runs 8 tests: one synthetic log per event type, with topic 0 the Keccak-256 of the canonical signature from the substreams-abis ABI JSON, asserting every decoded field, plus `common::transaction_tests!` (transaction, log and call metadata, the receipt fallback, reverted calls, failed transactions and a missing receipt). A differential run on 2026-10-13 fed 120,000 random synthetic blocks to this crate and to v0.8.0's (prost, substreams 0.7.6, substreams-abis v1.5.0): byte-identical `map_events` output and the same failing blocks (see the pull request). No captured-block fixture or live run backs this package yet.

## Build

From the repository root, with the pinned toolchain:

```sh
make -C dex/traderjoe build   # WASM, with the build.mk path remaps
make -C dex/traderjoe pack    # out/traderjoe-v0.9.0.spkg
```

Committed spkgs come from CI's Linux build, never a local pack.
