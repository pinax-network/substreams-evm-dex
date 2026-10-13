# CurveFi events

CurveFi pool (StableSwap), factory, CryptoSwap and CryptoSwap factory events, plus `Init`: a pool deployed directly (not by a factory), decoded from its StableSwap constructor arguments. Each log is decoded as the first event of the table it matches (v0.8.0's `continue`). `Init` is a synthetic log (no topics, no data; ordinal from the root CREATE call, block index 0, the CREATE call's metadata) placed before the transaction's decoded logs. A port of substreams-evm `dex/curvefi` at tag `evm-dex-v0.8.0` (`9663b25`) to substreams 0.8.0, buffa and substreams-abis v2.0.0. Its output is unchanged, including the v0.8.0 behaviour listed under [Kept from v0.8.0](#kept-from-v080).

`map_events(Block)` emits `curvefi.v1.Events`: per successful transaction with at least one decoded log, the transaction, then each decoded log with its raw fields (address, ordinal, topics, data, block index), its emitting call (when the block has call traces; otherwise the receipt's logs, with no call) and the decoded event. Every emitter counts: nothing checks that a log comes from a known contract.

## Events

| Event | `substreams-abis` module | `Log.log` |
| --- | --- | --- |
| `TokenExchange(address,int128,uint256,int128,uint256)` | `curvefi::pool` | `TokenExchange` |
| `AddLiquidity(address,uint256[3],uint256[3],uint256,uint256)` | `curvefi::pool` | `AddLiquidity` |
| `RemoveLiquidity(address,uint256[3],uint256[3],uint256)` | `curvefi::pool` | `RemoveLiquidity` |
| `RemoveLiquidityOne(address,uint256,uint256)` | `curvefi::pool` | `RemoveLiquidityOne` |
| `RemoveLiquidityImbalance(address,uint256[3],uint256[3],uint256,uint256)` | `curvefi::pool` | `RemoveLiquidityImbalance` |
| `CommitNewAdmin(uint256,address)`, `NewAdmin(address)` | `curvefi::pool` (CryptoSwap's alike) | `CommitNewAdmin`, `NewAdmin` |
| `CommitNewFee(uint256,uint256,uint256)`, `NewFee(uint256,uint256)` | `curvefi::pool` | `CommitNewFee`, `NewFee` |
| `RampA(uint256,uint256,uint256,uint256)`, `StopRampA(uint256,uint256)` | `curvefi::pool` | `RampA`, `StopRampA` |
| `PlainPoolDeployed(address[],uint256,uint256,address)` | `curvefi::factory` | `PlainPoolDeployed` (pool: the transaction's first CREATE) |
| `MetaPoolDeployed(address,address,uint256,uint256,address)` | `curvefi::factory` | `MetaPoolDeployed` (likewise) |
| `BasePoolAdded(address)`, `LiquidityGaugeDeployed(address,address)` | `curvefi::factory` | `BasePoolAdded`, `LiquidityGaugeDeployed` |
| `TokenExchange(address,uint256,uint256,uint256,uint256)` | `curvefi::cryptoswap` | `CryptoswapTokenExchange` |
| `AddLiquidity(address,uint256[3],uint256,uint256)`, `RemoveLiquidity(address,uint256[3],uint256)`, `RemoveLiquidityOne(address,uint256,uint256,uint256)`, `ClaimAdminFee(address,uint256)` | `curvefi::cryptoswap` | `CryptoswapAddLiquidity`, `CryptoswapRemoveLiquidity`, `CryptoswapRemoveLiquidityOne`, `CryptoswapClaimAdminFee` |
| `CommitNewParameters(uint256 x8)`, `NewParameters(uint256 x7)`, `RampAgamma(uint256 x6)`, `StopRampA(uint256,uint256,uint256)` | `curvefi::cryptoswap` | `CryptoswapCommitNewParameters`, `CryptoswapNewParameters`, `CryptoswapRampAgamma`, `CryptoswapStopRampA` |
| `CryptoPoolDeployed(address,address[2],uint256 x10,address)` | `curvefi::cryptoswapfactory` | `CryptoPoolDeployed` (pool: the transaction's first CREATE) |
| `LiquidityGaugeDeployed(address,address,address)`, `TransferOwnership`, `UpdateFeeReceiver`, `UpdateGaugeImplementation`, `UpdatePoolImplementation`, `UpdateTokenImplementation` (each `(address,address)`) | `curvefi::cryptoswapfactory` | `Cryptoswapfactory*` |
| StableSwap constructor `(address,address[3],address,uint256,uint256,uint256)`, the last 256 bytes of a contract creation's input | `curvefi::stableswap::constructor` | `Init` (synthetic) |

Amounts are exact decimal strings; addresses and hashes are raw bytes.

## Host API

`curvefi::project(&eth::Block) -> Result<pb::Events, Error>` runs the same decoding without the Substreams runtime, for a host program such as `firehose-parquet` that decodes blocks itself. `eth` and `pb` re-export the input and output types. The `map_events` handler is compiled only for wasm32.

`project` fails the block where v0.8.0 panicked: a successful transaction without a receipt.

## Kept from v0.8.0

The port reproduces v0.8.0's output exactly, including behaviour that looks wrong; changing it is a separate, owner-approved change.

1. **`Init` for nearly every contract creation.** v0.8.0 decodes the last 256 bytes of any successful transaction with a root CREATE call (depth 0) as a StableSwap constructor. `ethabi` accepts any 256 bytes there (it does not check address padding), so every deployment with at least 256 bytes of input emits an `Init` with arbitrary coins, not only Curve pools; v0.8.0's README claims a padding check its code does not make.
2. **Factory deployments name the first CREATE.** `PlainPoolDeployed`, `MetaPoolDeployed` and `CryptoPoolDeployed` take the address of the transaction's first CREATE call (any depth, reverted or not); a deployment without one is matched but dropped.
3. **`Transaction.gas_used`** is the receipt's cumulative gas, not the transaction's own.
4. **`StorePool`** is declared in the proto but never emitted: v0.8.0 has no store module.

## Port from v0.8.0

Every source file comes from substreams-evm `9663b25` (tag `evm-dex-v0.8.0`):

| File | Source | Changes |
| --- | --- | --- |
| `src/lib.rs` | `dex/curvefi/src/lib.rs` | `map_events` body as `project(&Block)`, with `map_events` in a wasm32-only module; receipt check up front; buffa: oneof variants built with `Log::from` |
| `src/tests.rs` | `dex/curvefi/src/lib.rs` (`mod tests`) | v0.8.0's seven tests, adapted to buffa (`process_block` is `project`); new tests for every event type |
| `substreams.yaml` | `dex/curvefi/substreams.yaml` | version v0.9.0, this repository's URL, no image; same module, input and output |
| `Makefile` | `dex/curvefi/Makefile` | `build` and `pack` only, with `build.mk`'s path remaps; packs to `out/` |
| `Cargo.toml` | `dex/curvefi/Cargo.toml` | version 0.9.0; also an `rlib`; `common`'s `testing` feature for tests |
| `../../proto/v1/dex/curvefi.proto` | `proto/v1/dex/curvefi.proto` | none (byte-identical) |
| `../../common/src/create.rs` | `common/src/create.rs` | one macro implements the templates for `dex.swaps.v1` and every per-protocol package; field values unchanged |

v0.8.0's `dex/curvefi/README.md` described the module as TRON-only and claimed the `Init` decoding validates address padding; this README replaces it.

Differences from v0.8.0:

- Where v0.8.0 panicked, `project` returns an error; the block fails either way.
- substreams-abis v2.0.0's readers replace v1.5.0's `ethabi` decoding and keep its accept/reject rules; every event above has the same topic 0, fields and `match_log` rules (topic count, data length) in both versions.
- On wasm32 only (by inspection of `ethabi` 17.2's decoder, not exercised): v1.5.0's `ethabi` adds offsets and lengths read from the log in a 32-bit `usize` without a check, so a `PlainPoolDeployed` log whose offset or length makes that sum wrap (an offset within 32 of `u32::MAX`, for one) made v0.8.0 panic, failing the block; v2.0.0's readers reject the log, so the port skips it. On a 64-bit host both reject it, which is what the differential run below compares.
- `PlainPoolDeployed.A` and `MetaPoolDeployed.A` keep their proto name in Rust (prost had `a`); the wire format is unchanged.

## Validation

`cargo test -p curvefi` runs 26 tests: one synthetic log per event type, with topic 0 the Keccak-256 of the canonical signature from the substreams-abis ABI JSON, asserting every decoded field, plus `common::transaction_tests!` (transaction, log and call metadata, the receipt fallback, reverted calls, failed transactions and a missing receipt). A differential run on 2026-10-13 fed 120,000 random synthetic blocks to this crate and to v0.8.0's (prost, substreams 0.7.6, substreams-abis v1.5.0): byte-identical `map_events` output and the same failing blocks (see the pull request). No captured-block fixture or live run backs this package yet.

## Build

From the repository root, with the pinned toolchain:

```sh
make -C dex/curvefi build   # WASM, with the build.mk path remaps
make -C dex/curvefi pack    # out/curvefi-v0.9.0.spkg
```

Committed spkgs come from CI's Linux build, never a local pack.
