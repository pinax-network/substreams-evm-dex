# DEX pools

Pool metadata (`tokens[]` plus `factory` when known) from DEX pool creations,
kept in a `set_if_not_exists` store that `dex/swaps` reads. A port of
substreams-evm `dex-swaps-pools` at tag `evm-dex-v0.8.0` (`9663b25`) to
substreams 0.8.0, buffa and substreams-abis v2.0.0. Its output is unchanged
for every block v0.8.0 accepts; see the deviations below for the blocks it
failed.

| Module | Kind | Output |
| --- | --- | --- |
| `store_pools` | store, `set_if_not_exists`, `proto:dex.foundational_store.v1.Pool` | key: hex pool address (lowercase, no `0x`); value: `Pool { tokens[], factory }` |
| `map_pools` | map over `store_pools` deltas | `dex.foundational_store.v1.Pools`: the pools first stored in the block |

The first writer wins: a pool is stored once and never overwritten, so
`map_pools` emits each pool in the block that first creates it.

## Pool creations

| Protocol | Event or source | Key | `tokens` | `factory` |
| --- | --- | --- | --- | --- |
| Uniswap V1 | `NewExchange` | exchange | `[token]` | emitter |
| Uniswap V2 | `PairCreated` | pair | `[token0, token1]` | emitter |
| Uniswap V3, Kyber Elastic | `PoolCreated` (one signature) | pool | `[token0, token1]` | emitter |
| Uniswap V4 | `Initialize` | 32-byte pool id | `[currency0, currency1]` | emitter (PoolManager) |
| Aerodrome | `PoolCreated` | pool | `[token0, token1]` | emitter |
| TraderJoe | `LBPairCreated` | LB pair | `[tokenX, tokenY]` | emitter |
| Balancer V3 | `PoolRegistered` | pool | token of each `tokenConfig` | the event's `factory` |
| Bancor | `NewConverter` (both factories), `FeaturesAddition` | converter, address | `[]` | emitter |
| SunPump | `TokenCreate` (v1 and legacy launchpad) | token | `[]` | emitter |
| CurveFi factories | `PlainPoolDeployed`, `MetaPoolDeployed`, `CryptoPoolDeployed` | the transaction's first CREATE | coins | emitter |
| CurveFi direct StableSwap | constructor arguments at the end of a deployment's input | the root CREATE | the three coins | empty |

Logs come from the successful transactions' non-reverted calls, or from the
receipt when a transaction has no call traces. A log yields at most one pool,
tried in the order above. Matching an event signature does not authenticate the
emitter: any contract emitting a matching event creates a pool entry.

## Host API

`dex_pools::project(&eth::Block) -> Result<Pools, Error>` runs the same
extraction without the Substreams runtime, for a host program such as
`firehose-parquet` that decodes blocks itself. It returns the block's pools in
store write order, first write per pool kept: what `map_pools` emits when the
store holds none of them yet. The host drops the pools it already holds.
`pools_from_deltas` is `map_pools` itself. `eth` re-exports the Firehose block
types.

## Port from v0.8.0

Every source file comes from substreams-evm `9663b25` (tag `evm-dex-v0.8.0`):

| File | Source | Changes |
| --- | --- | --- |
| `src/lib.rs` | `dex-swaps-pools/src/lib.rs` | handlers in a wasm32-only module; extraction moved into `project` and `pools_from_deltas` for the host; `CallType` comparisons on buffa's `EnumValue`; a missing receipt is an error instead of a panic |
| `src/curvefi.rs` | `dex-swaps-pools/src/curvefi.rs` | `CallType` comparisons on buffa's `EnumValue` |
| `src/{aerodrome,balancer,bancor,kyber,sunpump,traderjoe,uniswap}.rs` | same names | none |
| `substreams.yaml` | `dex-swaps-pools/substreams.yaml` | package `dex_pools` (was `dex_swaps_pools`), version v0.9.0, repository paths; same modules |
| `../../proto/v1/dex/foundational-store.proto` | `proto/v1/dex/foundational-store.proto` | none (byte-identical) |

Deviations, none of which changes the output of a block v0.8.0 accepts:

- A successful transaction with neither call traces nor a receipt fails the
  block with an error. v0.8.0 panicked there, which also failed the block.
- substreams-abis v1.5.0 decoded with `ethabi`; v2.0.0 has its own readers,
  which keep `ethabi`'s accept/reject rules on a 64-bit host. Every event used
  here has the same topic 0 and `match_log` rules (topic count, data length) in
  both versions.
- On wasm32, where v0.8.0 ran, ethabi's bounds check wraps for an offset or
  length word near `u32::MAX`: `store_pools` panicked on such a Balancer V3
  `PoolRegistered`, Curve `PlainPoolDeployed` or SunPump legacy `TokenCreate`
  and failed the block. The readers reject the field and the log yields no
  pool, so the block's other pools are still stored. The owner accepted this
  difference on 2026-10-25 (see
  [`dex/swaps`](../swaps/README.md#accepted-differences-from-v080)).

## Validation

`cargo test -p dex-pools` builds synthetic logs whose topic 0 is the Keccak-256
of the canonical event signature, for every collector above, and checks
`project` against the store and map modules on an empty store. A differential
run on 2026-10-13 compared this crate with v0.8.0's collectors on 120,000
random synthetic blocks (see the pull request): identical `map_pools` output
and store contents. It ran natively on a 64-bit host, where the wasm32 panics
above cannot occur. The released-WASM corpus of
[`dex/swaps`](../swaps/README.md#immutable-released-wasm-comparison) covers
them, and compares the released `store_pools` with `project` for every
collector. No captured-block fixture or live run backs this package yet.

## Build

From the repository root, with the pinned toolchain:

```sh
make -C dex/pools build   # WASM, with the build.mk path remaps
make -C dex/pools pack    # out/dex-pools-v0.9.0.spkg
```

Committed spkgs come from CI's Linux build, never a local pack.
