# Released EVM DEX v0.8.0 oracle

The exact public [evm-dex-v0.8.0 release](https://github.com/pinax-network/substreams-evm/releases/tag/evm-dex-v0.8.0)
asset `evm-clickhouse-dex-v0.8.0.spkg` supplies the three historical binaries.
No current source is rebuilt to stand in for the old decoder.

- Package: 1,454,120 bytes; SHA-256 `761fe2c6b2d46369781730d53962f0a348fd0df0faf3943f5979e8a2316d11d2`.
- `db_out` module hash with packaged `hex` params: `55c511daf545d0315295b98c2eb0838c2420bd8e`.
- `db:dex_swaps:map_events` module hash: `4673bc775b026a055eb6b76b0b8622515049dbd1`.
- `db:dex_swaps:dex_swaps_pools:store_pools` module hash: `c8c9d7f8c649b83cacba094f7ae550a37ad9defe`.
- Frozen corpus SHA-256: `6da45fa8b08f31b26c9ea8a9319c6084ff1439fa7cc060faa6c03ab14b72b5ed`.

| Binary | Bytes | SHA-256 |
| --- | ---: | --- |
| `db_out.wasm` | 250040 | `6e3fb029b9511fac79a732f391099521ab6e560b5519de62f3ec7621208dabe4` |
| `dex_swaps.wasm` | 660594 | `bbc351be2e79ab90e9a265dc07370804ec63f1fc82150c0bbbd35851e7455016` |
| `store_pools.wasm` | 319713 | `0b71ff6162bce3fe6da4b29aea2333ac957bc10325405866b4de1dd0935be658` |

`extract_oracle.py /path/to/evm-clickhouse-dex-v0.8.0.spkg` validates the exact
package, module names, binary indexes, entrypoints and binary hashes. It never
downloads anything or replaces a differing existing fixture.

The test in `src/legacy_v080_tests.rs` executes the actual pool store, swap map
and row formatter. It compares public `dex_pools::project` updates applied first
writer wins to the historical store, then compares all protobuf swap/fee/init
events and database row fields, keys, operations, ordinals and order in both
`hex` and `tron_base58` encodings. Composite keys and fields compare by name,
independently of map serialization order. The frozen raw database outputs retain
the historical keys; the test compares them directly with the native formatter.

The 73 synthetic cases cover all 14 supported swap protocols with known pools,
missing metadata, empty token vectors and malformed token widths; stateless
protocols; receipt fallback and receipt/call preload disagreement; failed or
reverted execution; maximum integer amounts and ambiguous directions; creation
before swaps and first-writer preservation; V4 initialization lookup behavior;
and combined protocol order and transaction/call fields. The two rejected cases
are missing receipts and overflowing Curve fees: historical WASM panics and the
native API returns an error, both rejecting the block. Every other case agrees.
No decoder behavior is corrected, omitted or waived by the test.

The test-only Wasmi 0.46 host supplies exactly the imported SDK functions,
including `state.get_last` over the block's post-store state and
`state.set_if_not_exists`. There is no fabricated ordinal-history lookup. It
bounds memory at 64 MiB, fuel at one billion per module, input/output at 4 MiB,
store keys at 64 bytes, pool values at 4096 bytes, entries at 4096 and store calls
at 16,384. Every module gets a fresh instance. Host, import, memory and fuel
failures fail the test, independently of registered decoder panics. No network,
RPC, filesystem, WASI or external store is exposed to WASM. Nothing is linked
into production dependencies.

`cargo test --locked -p dex-swaps released_v080` reruns the old WASM and validates
the frozen inputs, state and outputs without rewriting them. An intentional,
reviewed corpus change can use `UPDATE_EVM_DEX_V080_CORPUS=1` with that command;
review the bytes, outcomes and updated hash. Empty protobuf outputs use empty
byte arrays, including when decoding the frozen hex representation.

This is an offline v0.8.0 compatibility corpus, not live qualification, a
comparison with Token API v0.5.0 sinks, proof of every protocol variant or an
approval of the known v0.8.0 direction/metadata quirks. Captured-block evidence,
provider coverage and actual deployment checks remain separate.
