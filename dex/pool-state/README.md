# Complete-block pool state

One RPC-free `map_events(Block)` emits `dex.pool_state.v1.BlockPoolState` from
Firehose **Extended** blocks, including complete blocks without matching logs.
It extracts V2 and V3 changes together; there are no imported maps, stores,
database-change modules or price calculations.

- `v2_pools` retains each pool's final successful Sync reserves, including zero
  reserves. A malformed matching Sync invalidates that pool for the entire block,
  even if a later Sync is valid. Invalid reserves are empty strings and
  `invalid` is true; consumers must discard cached reserve state.
- `v3_pools` retains all Initialize, Swap, Mint, Burn and invalid markers in
  canonical block-log order. Trailing liquidity changes and negative ticks are
  preserved. The last Swap is not a complete closing-liquidity snapshot.
- Successful transaction call logs are used once, excluding state-reverted calls.
  Failed/reverted transactions do not change the output. Receipts are never used
  as a fallback. Base blocks (including empty ones), unknown detail levels,
  unknown transaction statuses and successful transactions without call traces
  are rejected. Block/header identity and nanosecond timestamp bounds are checked.

The output carries raw integer decimal strings without floating-point conversion.
Matching an event signature does not authenticate its emitting contract. Consumers
must independently verify pool/token identity, establish V3 initial state and
maintain uninterrupted canonical block continuity. This package does not supply
decimals, USD conversion, price policy, admission or a TWAP.

## Migration and compatibility

This package replaces `substreams-evm/dex-pool-state` and its separate V2/V3
closing-state modules. The v0.2.0 package uses the canonical `map_events` entry
point and intentionally requires Extended data; the old `map_pool_state` entry
point and Base receipt fallback are not retained. Consumers must select the new
module and pin the new package digest.

The protobuf package names, message names, field numbers, field types and oneof
tags are unchanged: `dex.pool_state.v1`, `uniswap.v2` and `uniswap.v3`. Only the
pool-state messages live in this repository; unrelated swap/event projections
were not migrated. Contract ABI sources and generated Rust structs remain in
the shared `substreams-abis` dependency, reused at the workspace's existing
v1.5.0 pin (`dex::uniswap::v2::pair::events::Sync` and the V3 pool events).
All ABI topics must be full 32-byte words before V3 decoding.

The migrated extraction/tests originate from `substreams-evm` revision
`efa22ed`. Historical package and live evidence apply to that original build,
not automatically to this Extended-only package. The new package has no live
qualification, and the existing live-testing hold remains in force.

## Offline validation and packaging

From the repository root, using its pinned Rust toolchain:

```sh
cargo test --offline --locked -p dex-pool-state
cargo clippy --offline --locked -p dex-pool-state --all-targets -- -D warnings
cargo build --offline --locked --release --target wasm32-unknown-unknown -p dex-pool-state
mkdir -p out/pool-state
substreams pack dex/pool-state/substreams.yaml -o out/pool-state/dex-pool-state-v0.2.0.spkg
```

Packing uses local protobufs and the local WASM only; no upstream source or RPC
connection is needed. Generated packages stay under ignored `out/` unless a
separately reviewed release explicitly adds one.
