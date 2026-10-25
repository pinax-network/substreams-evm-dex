# EVM DEX foundational store

Foundational-store entries for DEX pools: each pool's tokens and factory,
keyed by the pool. A port of substreams-evm `dex-swaps-pools-foundational-store`
(crate `evm-dex-foundational-store`, package `evm_dex_foundational_store`) at
tag `evm-dex-v0.8.0` (`9663b25`) to substreams 0.8.0 and buffa. Its output is
unchanged for every block v0.8.0 accepts, including the v0.8.0 behaviour listed
under [Kept from v0.8.0](#kept-from-v080); the blocks its inputs failed on
malformed dynamic ABI data are an
[accepted difference](../swaps/README.md#accepted-differences-from-v080) (see
[Port from v0.8.0](#port-from-v080)). The package keeps v0.8.0's name, so a
foundational store deployed from it is `evm_dex_foundational_store@v0.9.0`.

`map_entries` reads the `map_events` output of eleven per-protocol packages of
this repository, imported as local manifests, and emits
`sf.substreams.foundational_store.model.v2.SinkEntries`: one `Entry` per pool
creation, whose key is the pool and whose value is an `Any` holding a
`dex.foundational_store.v1.Pool { tokens, factory }`, with `if_not_exist` set
(the first writer of a key wins). A block without a pool creation gives an
empty `SinkEntries` (`if_not_exist` unset). [`dex/pools`](../pools/README.md)
keeps the same metadata in a Substreams store instead.

## Entries

In this order (the inputs' order, then the events'):

| Package | Event | Key | Tokens | Factory |
| --- | --- | --- | --- | --- |
| `dex/sunpump` | `TokenCreate`, `TokenCreateLegacy` | token | none | emitter |
| `dex/balancer` | `PoolRegistered` | pool | `token_config` tokens | the event's factory |
| `dex/bancor` | `FeaturesAddition` | its address | none | emitter |
| `dex/bancor` | `NewConverter` | converter | none | emitter |
| `dex/curvefi` | `Init` | pool | coins | none |
| `dex/curvefi` | `PlainPoolDeployed`, `CryptoPoolDeployed` | pool | coins | emitter |
| `dex/curvefi` | `MetaPoolDeployed` | pool | the coin | emitter |
| `dex/aerodrome` | `PoolCreated` | pool | token0, token1 | emitter |
| `dex/traderjoe` | `LbPairCreated` | pair | token X, token Y | emitter |
| `dex/kyber-elastic` | `PoolCreated` | pool | token0, token1 | emitter |
| `dex/uniswap-v1` | `NewExchange` | exchange | the token | emitter |
| `dex/uniswap-v2` | `PairCreated` | pair | token0, token1 | emitter |
| `dex/uniswap-v3` | `PoolCreated` | pool | token0, token1 | emitter |
| `dex/uniswap-v4` | `Initialize` | pool id | currency0, currency1 | emitter |

An event with an empty key is skipped. Protocol-specific fields (Aerodrome
`stable`, TraderJoe `bin_step`, Kyber `swap_fee_units`/`tick_distance`, ...)
stay on the pool-creation events.

## Host API

`evm_dex_foundational_store::project(&ProtocolEvents) -> SinkEntries` builds
the entries without the Substreams runtime. `ProtocolEvents` holds one
block's outputs of the eleven packages, which a host such as
`firehose-parquet` fills from each package's `project(&eth::Block)`. `Entry`,
`Key`, `SinkEntries` and `POOL_TYPE_URL` are re-exported; the `map_entries`
handler is compiled only for wasm32. The handler fails the block when an
input does, as Substreams does.

## Kept from v0.8.0

The port reproduces v0.8.0's output exactly for every block v0.8.0 accepts,
including behaviour that looks wrong; changing it is a separate, owner-approved
change.

1. **Kyber Elastic and Uniswap V3 share `PoolCreated`.** Each V3 or Kyber
   pool creation gives two entries with the same key, Kyber's first; the
   first writer wins, so the pool's entry is Kyber's (with the same tokens and
   factory).
2. **CurveFi `Init` for nearly every contract creation.** `dex/curvefi` emits
   an `Init` for any deployment whose input ends in 256 bytes (see its
   README), so such deployments get an entry keyed by the deployed contract,
   with arbitrary coins and no factory.
3. **Emitters are not authenticated.** Any contract emitting a matching event
   adds an entry, Bancor's `FeaturesAddition` included.

## Port from v0.8.0

Every source file comes from substreams-evm `9663b25` (tag `evm-dex-v0.8.0`):

| File | Source | Changes |
| --- | --- | --- |
| `src/lib.rs` | `dex-swaps-pools-foundational-store/src/lib.rs` | `map_entries` body as `project(&ProtocolEvents)`, with `map_entries` in a wasm32-only module; buffa: the value through `Any::pack` (same type URL and bytes), `Entry` fields through `.into()`; `POOL_TYPE_URL` constant |
| `src/tests.rs` | `dex-swaps-pools-foundational-store/src/lib.rs` (`mod tests`) | v0.8.0's two tests, adapted to buffa; new tests, one through the per-protocol packages' `project` |
| `substreams.yaml` | `dex-swaps-pools-foundational-store/substreams.yaml` | version v0.9.0, this repository's URL, no image; imports the per-protocol packages' local manifests instead of their v0.6.0 spkgs; same module, inputs, output and descriptor set |
| `Makefile` | `dex-swaps-pools-foundational-store/Makefile` | builds the eleven imported packages and this one, one cargo build each, with `build.mk`'s path remaps; `pack` to `out/` only |
| `Cargo.toml` | `dex-swaps-pools-foundational-store/Cargo.toml` | version 0.9.0; no prost; the per-protocol crates as dev-dependencies for tests |
| `../../proto/v1/dex/foundational-store.proto` | `proto/v1/dex/foundational-store.proto` | none (already in this repository, byte-identical) |

v0.8.0 imported the per-protocol packages' committed v0.6.0 spkgs. Those were
packed at substreams-evm `f767e6c`, where the packages' sources and
dependencies equal v0.8.0's, so importing the ported packages keeps the inputs.

One difference: on wasm32, where v0.8.0 ran, the `dex/balancer`, `dex/curvefi`,
`dex/sunpump` and `dex/traderjoe` inputs failed the block on a log whose
dynamic ABI offset or length wraps 32 bits (see their READMEs), and
`map_entries` failed with them. The ported inputs skip that log, so
`map_entries` writes the block's other entries. The owner accepted this
difference on 2026-10-25 (see [`dex/swaps`](../swaps/README.md#accepted-differences-from-v080)).

## Validation

`cargo test -p evm-dex-foundational-store` runs 5 tests. The 2026-10-13
differential run (120,000 random synthetic blocks, see the pull request) also
compared `map_entries` on the eleven packages' outputs with v0.8.0's (prost,
substreams 0.7.6): byte-identical, and failing where an input failed. It ran
natively on a 64-bit host, where the wasm32 failures above cannot occur. No
captured-block fixture or live run backs this package yet.

## Build

From the repository root, with the pinned toolchain:

```sh
make -C dex/foundational-store build   # the eleven imported WASMs, then this one
make -C dex/foundational-store pack    # out/evm-dex-foundational-store-v0.9.0.spkg
```

`pack` reads the descriptor set
`buf.build/streamingfast/substreams-foundational-store` from the Buf Schema
Registry. Committed spkgs come from CI's Linux build, never a local pack.
