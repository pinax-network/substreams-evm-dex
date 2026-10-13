# DEX swaps

Swaps, swap-fee settings and pool initializations of EVM DEX protocols,
normalized to `dex.swaps.v1.Events`. A port of substreams-evm `dex-swaps` at
tag `evm-dex-v0.8.0` (`9663b25`), the unified-swaps design the owner chose on
2026-10-13, to substreams 0.8.0, buffa and substreams-abis v2.0.0. Its output
is unchanged, including the v0.8.0 behaviour listed under
[Kept from v0.8.0](#kept-from-v080).

`map_events(Block, dex_pools:store_pools)` reads pool metadata from the
[`dex/pools`](../pools/README.md) store (imported as a local manifest) and
emits, per successful transaction with at least one decoded log, the
transaction, then each decoded log with its raw fields, emitting call and one
of `Swap`, `SwapFee` or `Initialize`.

## Protocols

| Protocol | `Swap` from | `SwapFee` from | `Initialize` from | Needs the stored pool |
| --- | --- | --- | --- | --- |
| Uniswap V1 | `TokenPurchase`, `EthPurchase` (ETH as the zero address) | `NewExchange`: 3000 | `NewExchange` | swaps |
| Uniswap V2 | `Swap` | `PairCreated`: 3000 | `PairCreated` | swaps |
| Uniswap V3 | `Swap` | `PoolCreated` fee | `PoolCreated` | swaps |
| Uniswap V4 | PoolManager `Swap` (by pool id) | `Initialize` fee | `Initialize` | swaps; `Initialize` for its factory |
| Kyber Elastic | `Swap` | | `PoolCreated` | swaps |
| Aerodrome | `Swap` | | `PoolCreated` | swaps |
| TraderJoe | LB pair `Swap` | | `LBPairCreated` | swaps |
| CurveFi | `TokenExchange` (pool and CryptoSwap) | `CommitNewFee`, `NewFee`, factory deployments | `PlainPoolDeployed`, `MetaPoolDeployed` | swaps, fee changes |
| Balancer | V3 vault `Swap` | V2 `SwapFeePercentageChanged`, V3 `AggregateSwapFeePercentageChanged` (in bps) | V3 `PoolRegistered` | swaps, fee changes |
| Bancor | `Conversion` | `ConversionFeeUpdate` | `NewConverter` (both factories) | swaps, fee changes |
| SunPump | `TokenPurchased`, `TokenSold` (TRX as the zero address) | `PurchaseFeeSet`, `SaleFeeSet` | `TokenCreate` (v1, legacy) | swaps (by token) |
| CoW | settlement `Trade` | | | no |
| DODO | route proxy `OrderHistory` | | | no |
| WOOFi | `WooSwap` | | | no |

`SwapFee.fee` keeps each protocol's own unit. Amounts are exact decimal
strings. A swap whose pool is not stored, whose token index is out of range or
whose amounts do not give one input and one output is dropped.

## Host API

`dex_swaps::project(&eth::Block, &impl PoolLookup) -> Result<pb::Events, Error>`
runs the same decoding without the Substreams runtime, for a host program such
as `firehose-parquet` that decodes blocks itself. `PoolLookup::get_pool(address)`
returns the pool `dex/pools` stored for a pool address (Uniswap V4: the pool id;
SunPump: the token), including pools created in the same block, as the
Substreams store does. `HashMap<Vec<u8>, Pool>` implements it. `project` asks
once per key of the block: every receipt-log emitter, plus the pools named by V4
and Balancer swaps and the SunPump trade tokens. `eth`, `pb` and `Pool`
re-export the input and output types.

`project` fails the block where v0.8.0 panicked: a successful transaction
without a receipt, or a fee above `u64::MAX` in a Curve `CommitNewFee`,
`NewFee`, `PlainPoolDeployed` or `MetaPoolDeployed`, or a SunPump
`PurchaseFeeSet` or `SaleFeeSet` log.

## Kept from v0.8.0

The port reproduces v0.8.0's output exactly, including behaviour that looks
wrong. Each item has a test that pins it; changing one is a separate,
owner-approved change.

1. **Uniswap V3 and Kyber Elastic share `Swap` and `PoolCreated` signatures.**
   Every decoder runs on every log, so a swap in any stored pool emits a
   `UNISWAP_V3` swap (positive amount is the input, `user` is the transaction
   sender) and a `KYBER_ELASTIC` swap in the opposite direction (`user` is the
   event sender). A `PoolCreated` emits both protocols' `Initialize`.
2. **Uniswap V4 direction.** v0.8.0 reads V4 amounts like V3 (positive is the
   input). V4's `Swap` reports the swapper's balance delta, where the input is
   negative; substreams-evm `90dff5e` says so for its ClickHouse view. V4 swaps
   are probably reversed.
3. **TraderJoe amounts.** v0.8.0 reads the high 128 bits of `amountsIn` and
   `amountsOut` as X. Liquidity Book packs X in the low 128 bits, so a swap of X
   for Y is probably reported as Y for X, with the amounts kept.
4. **Curve factory deployments** name the pool after the transaction's first
   non-reverted call with an address, usually the root call, while `dex/pools`
   keys the pool by the transaction's first CREATE.
5. **Lookups.** A V4 `Initialize` gets its factory only when a V4 swap in the
   same block names the pool; a Balancer `AggregateSwapFeePercentageChanged`
   counts only when the pool otherwise appears in the block's lookups.
6. **`Transaction.gas_used`** is the receipt's cumulative gas, not the
   transaction's own.
7. **Emitters are not authenticated.** Creation events, CoW, DODO and WOOFi
   swaps and SunPump fee settings count from any contract. Since Curve
   deployment and SunPump fee events need no stored pool, any contract can fail
   a block (v0.8.0: panic; port: error) by emitting one with a fee above
   `u64::MAX`.

## Port from v0.8.0

Every source file comes from substreams-evm `9663b25` (tag `evm-dex-v0.8.0`):

| File | Source | Changes |
| --- | --- | --- |
| `src/lib.rs` | `dex-swaps/src/lib.rs` | `map_events` in a wasm32-only module over `project` and `PoolLookup`; receipt check up front; fallible decoders propagate errors |
| `src/logs.rs` | `dex-swaps/src/logs.rs` | lookups through `PoolLookup` instead of `StoreGetProto` |
| `src/utils.rs` | `dex-swaps/src/utils.rs` | adds `truncated_fee`, v0.8.0's `to_u64() as u32` without the panic |
| `src/{curvefi,sunpump,bancor}.rs` | same names | return `Result`; fees through `truncated_fee` |
| `src/{uniswap_v3,uniswap_v4,kyber_elastic}.rs` | same names | direction helpers take `&[u8]` |
| all protocol modules | same names | buffa: `Protocol` as `EnumValue`, boxed oneof variants via `Log::from` |
| `substreams.yaml` | `dex-swaps/substreams.yaml` | version v0.9.0, imports `../pools/substreams.yaml` as `dex_pools` instead of the v0.8.0 spkg; same module |
| `../../common/src/create.rs` | `common/src/create.rs` | the `dex.swaps.v1` templates, from one macro shared with the per-protocol packages; values unchanged |
| `../../common/src/lib.rs` | `common/src/lib.rs` | `bigint_to_u64` (the per-protocol packages add `bigint_to_i32` and `try_to_u64`) |
| `../../proto/v1/dex-swaps.proto` | `proto/v1/dex-swaps.proto` | none (byte-identical) |

The tests in `common/src/create.rs` port `common/tests/create_log_tests.rs`;
`logs.rs` keeps v0.8.0's key-encoding test. substreams-abis v2.0.0 replaces
v1.5.0's `ethabi` decoding with readers that keep its accept/reject rules;
every event used here has the same topic 0 and `match_log` rules in both
versions, and the decoders are only reached through `match_and_decode`.

## Validation

`cargo test -p dex-swaps` covers every decoder above with synthetic logs whose
topic 0 is the Keccak-256 of the canonical event signature, plus `project`'s
lookups, metadata, receipt fallback, reverted calls and failures. A
differential run on 2026-10-13 fed 120,000 random synthetic blocks to this
crate and to v0.8.0's decoders (prost, substreams 0.7.6, substreams-abis
v1.5.0): byte-identical `map_events` output, and the same blocks failing (see
the pull request). No captured-block fixture or live run backs this package
yet.

## Build

From the repository root, with the pinned toolchain:

```sh
make -C dex/swaps build   # dex/pools and dex/swaps WASM, with the build.mk path remaps
make -C dex/swaps pack    # out/dex-swaps-v0.9.0.spkg
```

Committed spkgs come from CI's Linux build, never a local pack.
