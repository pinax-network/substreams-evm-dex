# Repository guidance

EVM DEX Substreams: swaps and pool state from Firehose blocks. Created on
2026-10-13 at the owner's direction, split from
`pinax-network/substreams-evm-extended` (which keeps the balance packages)
and taking over the DEX code of `pinax-network/substreams-evm`.

## Scope and formats

- Every package emits **native protobuf** (`map_*` modules), so native
  Substreams ClickHouse/Postgres sinks can consume it directly later.
- For the Token API today, an `evm-dex` `db_out` writes the ClickHouse
  tables the Token API reads, with the SQL embedded in its manifest's `sink:`
  section. The design is substreams-evm's v0.8.0 unified swaps
  (`dex.swaps.v1`, tag `evm-dex-v0.8.0`, `9663b25`), chosen by the owner on
  2026-10-13. It was never deployed: check parity against the production
  v0.5.0 tables before the Token API switches.
- Every package exposes plain-Rust decoding (`project(...)`) that a host
  program such as `firehose-parquet` calls on a decoded `Block` without the
  Substreams runtime. Handlers compile only for wasm32
  (`#[cfg(target_arch = "wasm32")] mod handler`). Swaps need pool metadata
  across blocks, so their projection takes a pool lookup the caller provides.
- Contract ABIs and generated decoders come from `substreams-abis` (tag
  `v2.0.0`); do not vendor ABI code here.

## Provenance

- `dex/pool-state` and its protos were imported with their history from
  substreams-evm-extended (`main` at `cd85495`); commit messages name that
  repository's pull requests explicitly. The owner's hold on its live
  Substreams, Firehose and RPC checks stays in force until reauthorized.
- The swaps, pools and `db_out` code is ported from substreams-evm `main`
  (`333e8c8`) and tag `evm-dex-v0.8.0` (`9663b25`), from substreams 0.7.6,
  prost and substreams-abis v1.5.0 to the toolchain below. Record the source
  commit of every ported file in its package README. `dex/pools`, `dex/swaps`,
  `evm-dex` and the `common` crate keep v0.8.0's output exactly, including
  behaviour that looks wrong (the "Kept from v0.8.0" sections of
  `dex/swaps/README.md` and `evm-dex/README.md`); changing it is a separate
  change the owner approves.
- The per-protocol raw-event packages (`dex/<protocol>`) and
  `dex/foundational-store` are ported the same way, from tag
  `evm-dex-v0.8.0`, and likewise keep v0.8.0's output (each README's "Kept
  from v0.8.0").
- Both keep it for every block v0.8.0 accepts; the one exception is in
  [Accepted differences from v0.8.0](dex/swaps/README.md#accepted-differences-from-v080).

## Toolchain and validation

`rust-toolchain.toml` pins Rust 1.99.0 exactly (a point release: the WASM
embeds `/rustc/<commit>`). The workspace is on substreams 0.8.0,
substreams-ethereum 0.12.0 and buffa 0.9.2. `proto/src/pb` is generated
from `proto/v1` by `buf generate` in `proto/`; only `pb/mod.rs` is written by
hand. Package Makefiles build WASM with `--locked` and the `RUSTFLAGS` of
`build.mk`, which remap the checkout, `CARGO_HOME` and the Rust sources.

Offline validation, all with warnings denied: formatting, workspace tests,
Clippy for all targets on the host and wasm32, a wasm32 check and rustdoc.
Keep regression tests in Rust inside the package crates, with captured-block
fixtures; verify new work ad hoc and report the method and results in the
pull request.

## Releases and deployment

Releases are plain `v<version>` tags starting at `v0.9.0`, so the Token API
databases (`<chain>:evm-dex@<version>`) never collide with substreams-evm's
`evm-dex` v0.5–v0.8. The Token API live sink takes the release's spkg URL as
`SPKG_URL`. Committed spkgs come from CI's Linux build, never a local pack.

## Live use

Every live Substreams, Firehose, RPC or sink step needs the owner's explicit
OK for that step. Keep credentials in environment variables only, and never
write internal hostnames, endpoints or secrets into this public repository.
