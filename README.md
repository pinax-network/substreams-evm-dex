# substreams-evm-dex

EVM DEX [Substreams](https://substreams.dev): swaps and pool state from
Firehose blocks, as native protobuf and as the Token API's ClickHouse
tables.

| Package | Status | Output |
| --- | --- | --- |
| [`dex/pool-state`](dex/pool-state/README.md) | imported from substreams-evm-extended | `dex.pool_state.v1.BlockPoolState`: Uniswap V2 reserves and ordered V3 pool changes from Extended blocks |
| `dex/pools` | to port from substreams-evm | pool creations per protocol |
| `dex/swaps` | to port from substreams-evm | `dex.swaps.v1.Events`: swaps, fees and initializations of every supported protocol |
| `evm-dex` | to port from substreams-evm | `db_out` into the Token API DEX tables, with the ClickHouse SQL |

## Roadmap

1. Workspace, toolchain and CI; `dex/pool-state` and its protos.
2. Port `dex-swaps-pools` and `dex-swaps` from substreams-evm `v0.8.0` to
   substreams 0.8 and buffa, with captured-block fixtures and offline parity
   against the original.
3. Port `evm-dex` `db_out` and its ClickHouse SQL; check the Token API
   queries offline, then parity against the production tables.
4. Release `v0.9.0` and deploy per chain as `<chain>:evm-dex@v0.9.0`.

See [AGENTS.md](AGENTS.md) for scope, provenance and the rules of this
repository.

## License

[Apache-2.0](LICENSE)
