# Released EVM DEX v0.8.0 oracle

The exact public [evm-dex-v0.8.0 release](https://github.com/pinax-network/substreams-evm/releases/tag/evm-dex-v0.8.0)
asset `evm-clickhouse-dex-v0.8.0.spkg` supplies the three historical binaries.
No current source is rebuilt to stand in for the old decoder.

- Package: 1,454,120 bytes; SHA-256 `761fe2c6b2d46369781730d53962f0a348fd0df0faf3943f5979e8a2316d11d2`.
- `db_out` module hash with packaged `hex` params: `55c511daf545d0315295b98c2eb0838c2420bd8e`.
- `db:dex_swaps:map_events` module hash: `4673bc775b026a055eb6b76b0b8622515049dbd1`.
- `db:dex_swaps:dex_swaps_pools:store_pools` module hash: `c8c9d7f8c649b83cacba094f7ae550a37ad9defe`.
- Frozen corpus SHA-256: `82800f461596f77666cfe5ace4cad44559295f9a1cc34f93eaa0ea680445fe6c`
  (schema version 2, 108 cases).

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
the historical keys and operations: the test decodes each frozen output and
compares its rows, keys and operations with the corpus's `rows` and `row_keys`,
which must equal the live historical and native results, so editing a frozen
operation from CREATE to UPDATE fails the test.

The 108 synthetic cases cover:

- all 14 supported swap protocols with known pools, missing metadata, empty
  token vectors and malformed token widths, and the stateless protocols;
- pool creation followed by a swap that reads the new pool, on an empty store,
  for every `dex/pools` collector: Uniswap V1, V2, V3 (with Kyber Elastic's
  shared signature) and V4, Aerodrome, TraderJoe, Balancer V3, Bancor (both
  `NewConverter` events and `FeaturesAddition`), SunPump (v1 and legacy
  `TokenCreate`), and Curve (`PlainPoolDeployed` with a normal and a truncated
  fee, `MetaPoolDeployed`, `CryptoPoolDeployed` and a direct deployment);
- first-writer preservation in one transaction, across transactions and
  against a seeded pool;
- fee and initialization rows of every protocol that writes them, including
  Curve and SunPump fees above `u32::MAX`, a Balancer fee above `u64::MAX`
  (0 bps), Uniswap V3 and V4 fees above `u32::MAX` and `u64::MAX`, and the
  Balancer aggregate fee, kept when the pool's own fee logs or a swap make it
  a lookup key and dropped when only the vault's event names it;
- receipt fallback and receipt/call preload disagreement; failed, reverted and
  nested reverted execution; maximum integer amounts and ambiguous directions;
  V4 initialization lookup behavior; a pool with one token;
- combined protocol order and transaction/call fields, and a block with events
  in two transactions (`tx_index` 0 and 1) around a reverted transaction and
  one without events, the second a contract creation (empty `tx_to`) with
  nested delegate-call logs and large integer fields;
- dynamic ABI offsets and lengths that do not wrap 32 bits (huge, out of range,
  unaligned): both implementations skip the log.

Each case records its `outcome`:

- `agree` (100 cases): same pool state, events and rows.
- `rejected` (2 cases, also `"rejected": true`): missing receipts and
  overflowing Curve fees. The historical WASM panics and the native API returns
  an error, both rejecting the block. `historical_panics` records the panic.
- `historical_rejects_native_skips_log` (6 cases): the accepted difference
  below. `historical_panics` records the `store_pools` and `map_events`
  panics; `control_block_hex` is the same block without the malformed log, and
  `post_store`, `oracle_events_hex` and `formatted` are the historical results
  for that control block. The native results for `block_hex` must equal them.

### Accepted difference: malformed dynamic ABI data

The owner, Denis Carriere, decided on 2026-10-25: "Not sure, I don't think we
should replicate block failure, we can accept the difference if this resolves
an issue". The released WASM's ethabi 17.2 checks `offset + len` in 32-bit
`usize`, so an offset or length word near `u32::MAX` wraps, passes the check
and panics on the slice. The native substreams-abis v2.0.0 readers reject the
field, and the native path skips the log and keeps the block. The test asserts
each historical panic message and location and the native result:

| Case | Malformed word | Historical `store_pools` | Historical `map_events` | Native |
| --- | --- | --- | --- | --- |
| `cow_uid_length_wraps` | CoW `Trade` `orderUid` length `0xffffffff` | accepts | `slice index starts at 224 but ends at 223` (`ethabi-17.2.0/src/decoder.rs:87:18`) | no events |
| `cow_uid_offset_wraps` | CoW `Trade` `orderUid` offset `0xffffffe0` | accepts | `slice index starts at 4294967264 but ends at 0` (`decoder.rs:71:17`) | no events |
| `balancer_tokens_offset_wraps` | Balancer V3 `PoolRegistered` token configs offset `0xffffffe0` | same panic as `map_events` | `slice index starts at 4294967264 but ends at 0` (`decoder.rs:71:17`) | no pool, no events |
| `curve_coins_offset_wraps` | Curve `PlainPoolDeployed` coins offset `0xfffffff0` | same panic as `map_events` | `slice index starts at 4294967280 but ends at 16` (`decoder.rs:71:17`) | no pool, no events |
| `sunpump_legacy_name_length_wraps` | SunPump legacy `TokenCreate` name length `0xffffffff` | same panic as `map_events` | `slice index starts at 224 but ends at 223` (`decoder.rs:87:18`) | no pool, no events |
| `cow_uid_length_wraps_beside_v2_swap` | the first case's `Trade`, beside a valid Uniswap V2 swap in another transaction | accepts | `slice index starts at 224 but ends at 223` (`decoder.rs:87:18`) | the swap: `swaps` and `blocks` rows the released package never wrote |

No other decoder behavior is corrected, omitted or waived by the test.

The test-only Wasmi 0.47.2 host supplies exactly the imported SDK functions,
including `state.get_last` over the block's post-store state and
`state.set_if_not_exists`. There is no fabricated ordinal-history lookup. It
bounds memory at 64 MiB, fuel at one billion per module, input/output at 4 MiB,
store keys at 64 bytes, pool values at 4096 bytes, entries at 4096, store calls
at 16,384 and registered panic text at 4096 bytes. Every module gets a fresh
instance. Host, import, memory and fuel failures fail the test, independently of
registered decoder panics, whose message and location the host returns. No
network, RPC, filesystem, WASI or external store is exposed to WASM. Nothing is
linked into production dependencies: CI fails if any workspace crate reaches
`wasmi` through a normal or build dependency on any target.

`cargo test --locked -p dex-swaps released_v080` reruns the old WASM and validates
the frozen inputs, state and outputs without rewriting them. An intentional,
reviewed corpus change can use `UPDATE_EVM_DEX_V080_CORPUS=1` with that command;
review the bytes, outcomes and updated hash. Empty protobuf outputs use empty
byte arrays, including when decoding the frozen hex representation.

This is an offline v0.8.0 compatibility corpus, not live qualification, a
comparison with Token API v0.5.0 sinks, proof of every protocol variant or an
approval of the known v0.8.0 direction/metadata quirks. Captured-block evidence,
provider coverage and actual deployment checks remain separate.
