# dex-pool-registry

A bounded native pool registry for `dex_pools::project` and
`dex_swaps::project`. It owns metadata and canonical undo history, not ABI
decoding, protocol authentication, token admission or pricing policy. There is
no Substreams handler, RPC client or seed downloader in this crate.

The existing projectors remain unchanged. Their documented v0.8.0 output and
lookup quirks still apply; this registry does not establish parity with any
deployed package or qualify a live source.

## Block transaction

```rust
use dex_pool_registry::{BlockRef, Completeness, Limits, Registry, Seed};

# fn example(block: &dex_pools::eth::Block) -> Result<(), Box<dyn std::error::Error>> {
let seed = Seed {
    chain: "ethereum".into(),
    revision: "reviewed-snapshot-1".into(),
    at: BlockRef { number: 100, hash: [1; 32] },
    completeness: Completeness::Partial,
    pools: vec![], // Supply the historical pool metadata required by the host.
};
let mut registry = Registry::new(seed, Limits::default())?;
let creations = dex_pools::project(block)?;
let prepared = registry.prepare_block(
    BlockRef { number: 101, hash: [2; 32] },
    [1; 32], // Verified parent hash of this block.
    &creations,
)?;
let events = dex_swaps::project(block, &prepared)?;
// Validate every downstream consumer before committing either side.
// An error/drop before commit leaves registry metadata and history unchanged.
prepared.commit();
# let _ = events;
# Ok(())
# }
```

`PreparedBlock` implements `PoolLookup`. It overlays all new creations on the
existing metadata without cloning the whole registry. It exposes `pool`,
`new_pools`, `len`, `block`, `seed_identity` and `completeness` for host-side
validation. `new_pools` contains only insertions, sorted by key. Its order is
not transaction/log order.

The caller verifies the chain identity, block hash, parent and complete block
envelope before preparing. The registry checks exact next height and parent
hash against its current head; it does not inspect or authenticate the block.
The exclusive borrow prevents concurrent or stale prepared commits. `commit`
has no recoverable failure; allocation exhaustion has ordinary Rust process
behavior.

## Metadata semantics and admission

- First writer wins in the seed, across blocks and within each creation batch.
  Later conflicting values do not replace the original metadata.
- Apply **every creation before projecting any swap from that block**. This
  matches the Substreams store's ordinal-zero writes and `get_last` lookup,
  even when a creation appears after a swap in the block.
- Feed the output of `dex_pools::project`, whose ordering preserves the
  existing collector precedence. The protocol-grouped foundational-store
  output has different collision precedence and is not an equivalent source.
- IDs must be lowercase, unprefixed hex representing 20 or 32 bytes. Uniswap
  V4 uses a 32-byte pool ID; SunPump uses its 20-byte token address.
- Token vectors may be empty or contain any count within the configured bound.
  Each token is 20 bytes; ordering, repeats and zero addresses are preserved.
  Factories may be empty or exactly 20 bytes. This covers direct Curve empty
  factories, V1/Curve Meta single coins, Bancor/SunPump empty token vectors and
  dynamic Curve/Balancer vectors without fabricating constituents.
- All supplied entries are validated, including duplicates that first-writer
  semantics would otherwise ignore. Raw entry counts also include duplicates.
  Missing protobuf pool values, malformed widths and noncanonical IDs fail
  atomically. The raw projectors accept some malformed Firehose identities;
  these stricter registry rules are explicit host admission boundaries.

`Pool` carries no protocol discriminator. A matching event signature and seeded
factory do not establish authenticated protocol identity. Balancer V2 has no
creation collector here; its historical metadata must be seeded. A Curve Meta
pool's single stored coin is not expanded into base-pool constituents. Missing
metadata and unsupported token indexes retain the swap decoder's existing
output behavior.

## Seed identity and completeness

The seed declares a caller-owned chain name, revision, exact anchor and
`Complete` or `Partial` historical metadata. Those labels describe the caller's
claim; the registry cannot verify snapshot completeness, lineage, protocol
coverage or factory authenticity. In particular, `Complete` does not assert
that every DEX or swap is supported. Empty metadata can still be declared
complete by a caller; callers must substantiate that declaration separately.

`seed_identity().fingerprint` is SHA-256 over the domain
`dex-pool-registry-seed-v1\0`, length-prefixed chain/revision, the big-endian
anchor height, 32-byte hash, completeness byte (`1` complete, `0` partial),
pool count and sorted effective first-writer entries. Each entry includes
length-prefixed raw key and factory, token count and length-prefixed tokens in
their original order. Lengths/counts are unsigned 64-bit big-endian. Equivalent
seed ordering produces the same fingerprint; a conflicting duplicate changes
it only if it changes the first value. This is content identity, not a signature.

The first ordinary block must be the seed's exact child. A host starting from
a later observation can instead use `prepare_anchor(at, creations, true)`:
it stages that observation's creations while the host suppresses publication.
It retains the original seed fingerprint, marks current completeness `Partial`
and makes the new anchor the undo floor. Same-height anchors require the exact
seed hash. Older anchors are refused. This operation is startup-only; neither
an undo back to the seed nor a later finality permits a second warm anchor.
`anchor_at` is the convenience form with no creations.

## Bounds, undo and finality

`Limits` bounds total pool count, raw creations per block, tokens per pool,
retained block frames and retained inserted pool keys. Seed raw entry count is
bounded by `max_pools`. Pools are never silently evicted to make capacity.
Exceeding a preparation bound fails the whole operation. A block requiring
more undo keys than the journal bound is refused; a warm anchor needs no undo
frame and is bounded by the other limits.

Every committed block, including an empty one, has an undo frame. Old frames
are evicted on commit as needed for either journal bound, retaining their pool
metadata. Undo requires an exact retained canonical target and removes only
keys first inserted after it. It cannot cross explicit finality, a warm anchor
or the retained-history floor. Unknown or conflicting targets fail atomically.
An undone branch's keys may be inserted afresh on its replacement branch.

`prepare_undo` and `prepare_finalize` validate and exclusively borrow the
registry, then offer infallible `commit`. Drop leaves it unchanged. Hosts can
therefore validate their own state changes before committing both sides.
`can_undo`, `can_finalize`, `undo_to` and `finalize` are convenience forms.
Finality is explicit, exact and monotonic; it prunes obsolete undo frames but
never removes pool metadata. The seed is an anchor, not implicitly finalized.

## Validation

Synthetic offline tests cover metadata shapes, first-writer precedence,
canonical identity admission, raw and retained bounds, atomic failure/drop,
seed fingerprint binding, warm anchors, empty blocks, reorg forks and finality.
An integration fixture runs the actual public pool and swap projectors with a
swap preceding its pool creation and compares complete `Events` to the
existing `HashMap` lookup. Another proves a swap projection error cannot commit
new metadata. These tests establish local registry behavior, not historical
seed completeness or live-source qualification.
