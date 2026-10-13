# substreams-evm-dex

EVM DEX [Substreams](https://substreams.dev): swaps, swap fees, pool
initializations, pool metadata and pool state from Firehose blocks. Every
package emits native protobuf, and `evm-dex` writes the Token API's ClickHouse
DEX tables. Every package is also a plain Rust library that a host program,
such as firehose-parquet, can call on a decoded block without the Substreams
runtime.

Sibling repositories:
[substreams-evm-balances](https://github.com/pinax-network/substreams-evm-balances)
(native and ERC-20 balances) and
[substreams-evm-transfers](https://github.com/pinax-network/substreams-evm-transfers)
(transfer events). This repository takes over the DEX code of
[substreams-evm](https://github.com/pinax-network/substreams-evm).

## Packages

| Package | Modules | Output |
| --- | --- | --- |
| [`dex/pools`](dex/pools/README.md) (`dex_pools` v0.9.0) | `store_pools`, `map_pools` | `dex.foundational_store.v1`: each pool's tokens and factory, stored first-writer-wins by pool address, and `Pools`, the pools first created in a block |
| [`dex/swaps`](dex/swaps/README.md) (`dex_swaps` v0.9.0) | `map_events` (imports `dex_pools`) | `dex.swaps.v1.Events`: `Swap`, `SwapFee` and `Initialize` per transaction, for every supported protocol |
| [`evm-dex`](evm-dex/README.md) (`evm_dex` v0.9.0) | `db_out` (imports `dex_swaps`) | `DatabaseChanges` into the Token API DEX tables; the ClickHouse SQL is embedded in the package |
| [`dex/pool-state`](dex/pool-state/README.md) (`dex_pool_state` v0.2.0) | `map_events` | `dex.pool_state.v1.BlockPoolState`: Uniswap V2 reserves and ordered V3 pool changes from Extended blocks |

```
sf.ethereum.type.v2.Block ──▶ dex_pools:store_pools ──┐
                 │                                    ▼
                 └──────────────────────────▶ dex_swaps:map_events ──▶ evm_dex:db_out ──▶ ClickHouse (Token API)
```

## Protocols

`dex.swaps.v1.Protocol`: Aerodrome, Balancer, Bancor, CoW, Curve, DODO, Kyber
Elastic, SunPump (Tron), TraderJoe, Uniswap V1, V2, V3 and V4, and WOOFi.
`dex/pools` records pool creations for the same list except CoW, DODO and
WOOFi, which have no pools. [`dex/swaps`](dex/swaps/README.md#protocols)
lists the events read for each protocol.

## ClickHouse tables (`evm-dex`)

| Object | Kind | Contents |
| --- | --- | --- |
| `swaps`, `swap_fee`, `initialize` | tables | one row per event, keyed by `(minute, timestamp, block_num, tx_index, log_index, block_hash)` |
| `blocks` | table | every block that wrote a row |
| `state_ohlc_prices`, `state_pools_aggregating_by_pool`, `state_pools_aggregating_by_token`, `state_pools_fees`, `state_pools_initialize`, `state_pools_uaw` | tables fed by materialized views | OHLC prices, per-pool and per-token aggregates, fees, initializations, unique active wallets |
| `ohlc_prices`, `ohlc_prices_uaw`, `pools_by_pool`, `pools_by_token` | views | what the Token API queries |

The SQL is substreams-evm `evm-dex-v0.8.0`'s, byte for byte, and `db_out`
writes the same rows. The `db_out` param is the address encoding: `hex`, or
`tron_base58` on Tron.

## Use from Rust

Each crate re-exports the block type it takes (`eth`) and its output types.
Swaps need pool metadata from earlier blocks, so the host keeps the pools,
first writer wins, and passes them in:

```rust
use std::collections::HashMap;
use dex_swaps::Pool;

let mut pools: HashMap<Vec<u8>, Pool> = HashMap::new();
for block in blocks {                       // decoded dex_swaps::eth::Block
    // Pools first created in this block. Store them before decoding its
    // swaps, as the Substreams store does.
    for created in dex_pools::project(&block)?.pools {
        let address = hex::decode(&created.address)?;
        let pool = created.pool.as_option().cloned().unwrap_or_default();
        pools.entry(address).or_insert(pool);
    }
    let events = dex_swaps::project(&block, &pools)?;                    // dex.swaps.v1.Events
    let rows = evm_dex::changes(&clock, &events, evm_dex::Encoding::Hex)?; // Token API rows
}
```

`dex_pool_state::project(&block)` needs no state across blocks.

## Build and test

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.0). The workspace
is on substreams 0.8.0, substreams-ethereum 0.12.0, buffa 0.9.2 and
substreams-abis `v2.0.0`.

```sh
cargo test --locked --workspace
make -C evm-dex build schema pack   # dex/pools, dex/swaps and evm-dex WASM; out/evm-dex-v0.9.0.spkg
```

CI runs formatting, tests, Clippy on the host and wasm32, a wasm32 check and
rustdoc, all with warnings denied. Release spkgs will come from CI's Linux
build: module hashes depend on the build host.

## Deploy

Releases are plain `v<version>` tags starting at `v0.9.0`. Each chain gets
its own database, `<chain>:evm-dex@<version>`; those names never collide
with substreams-evm's `evm-dex` v0.5–v0.8 databases. The Token API live sink
takes the release spkg as `SPKG_URL`. Production runs substreams-evm's
v0.5.0 today: check parity against its tables before the Token API switches.

## Provenance and status

- **Ported, output unchanged.** `dex/pools`, `dex/swaps`, `common` and
  `evm-dex` come from substreams-evm tag `evm-dex-v0.8.0` (`9663b25`). Each
  package README records its sources.
- **Checked offline only.** Differential tests against v0.8.0 run
  unmodified gave 0 mismatches: 120,000 synthetic blocks for pools and swaps,
  and 120,000 synthetic cases for `db_out`. The schema loads in
  clickhouse-local. No captured blocks yet, and nothing is live-qualified.
- **Questionable v0.8.0 behaviour kept for the owner to decide.** The lists
  are in [`dex/swaps`](dex/swaps/README.md#kept-from-v080) and
  [`evm-dex`](evm-dex/README.md#kept-from-v080). Examples: a Uniswap V3 swap
  also emits a Kyber Elastic swap, Uniswap V4 direction and TraderJoe X/Y look
  reversed, and `gas_used` is the receipt's cumulative gas.
- **`dex/pool-state`** came with its history from substreams-evm-balances
  (formerly substreams-evm-extended). The owner's hold on its live checks
  stays in force.

Next: the per-protocol raw-event packages, a CI job that checks committed
package hashes, a release workflow, captured-block fixtures and the `v0.9.0`
release.

See [AGENTS.md](AGENTS.md) for the rules of this repository.

## License

[Apache-2.0](LICENSE)
