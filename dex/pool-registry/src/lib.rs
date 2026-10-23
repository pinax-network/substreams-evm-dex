//! Bounded native pool metadata for [`dex_swaps::project`].
//!
//! Extract creations once with [`dex_pools::project`], prepare the entire block,
//! and pass the prepared lookup to the swap projector. Every creation in the
//! block is visible, including those after a swap. Commit only after all consumers
//! succeed; dropping a prepared operation leaves the registry unchanged.
//!
//! Metadata is first-writer-wins, without protocol authentication or token policy.
//! Accepted IDs are lowercase, unprefixed 20- or 32-byte hex; tokens are 20-byte
//! values, and factories are empty or 20-byte values. These are registry admission
//! rules, not claims about all malformed inputs accepted by the raw projectors.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

pub use dex_pools::{Pool, PoolCreated, Pools};
pub use dex_swaps::PoolLookup;
use sha2::{Digest, Sha256};

/// An exact block identity. The caller verifies the block's chain and envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockRef {
    pub number: u64,
    pub hash: [u8; 32],
}

/// Caller-declared historical metadata completeness, not protocol coverage,
/// authenticated factory identity, or a verification result from this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completeness {
    Complete,
    Partial,
}

/// Metadata at the parent of the first ordinary block. Duplicate IDs retain the
/// first value, including within the seed. Every supplied entry is validated.
#[derive(Clone, Debug)]
pub struct Seed {
    pub chain: String,
    pub revision: String,
    pub at: BlockRef,
    pub completeness: Completeness,
    pub pools: Vec<PoolCreated>,
}

/// Original seed provenance. The fingerprint binds chain, revision, anchor,
/// completeness and the effective first-writer metadata in sorted key order.
/// It does not authenticate the caller's declarations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedIdentity {
    pub chain: String,
    pub revision: String,
    pub at: BlockRef,
    pub completeness: Completeness,
    pub fingerprint: [u8; 32],
}

/// Memory and input bounds. Empty token vectors remain valid even though
/// `max_tokens_per_pool` must be positive. Old undo frames are evicted atomically
/// on commit when either history bound is exceeded; metadata remains stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_pools: usize,
    pub max_pools_per_block: usize,
    pub max_tokens_per_pool: usize,
    pub max_undo_blocks: usize,
    pub max_undo_pools: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pools: 65_536,
            max_pools_per_block: 4_096,
            max_tokens_per_pool: 256,
            max_undo_blocks: 256,
            max_undo_pools: 65_536,
        }
    }
}

/// Validation failure. Messages contain no caller-provided identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(&'static str);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
struct UndoFrame {
    block: BlockRef,
    parent: BlockRef,
    added: Vec<Vec<u8>>,
}

/// Native metadata and a bounded canonical undo journal. Seed entries and
/// first-writer metadata are never overwritten by later creations.
#[derive(Debug)]
pub struct Registry {
    seed: SeedIdentity,
    completeness: Completeness,
    pools: BTreeMap<Vec<u8>, Pool>,
    head: BlockRef,
    finalized: Option<BlockRef>,
    history: VecDeque<UndoFrame>,
    undo_pools: usize,
    started: bool,
    limits: Limits,
}

impl Registry {
    pub fn new(seed: Seed, limits: Limits) -> Result<Self> {
        check(
            limits.max_pools > 0 && limits.max_pools_per_block > 0 && limits.max_tokens_per_pool > 0 && limits.max_undo_blocks > 0 && limits.max_undo_pools > 0,
            "zero registry bound",
        )?;
        check(!seed.chain.trim().is_empty() && seed.chain.len() <= 256, "invalid seed chain")?;
        check(!seed.revision.trim().is_empty() && seed.revision.len() <= 256, "invalid seed revision")?;
        check(seed.pools.len() <= limits.max_pools, "seed entry bound exceeded")?;
        let mut pools = BTreeMap::new();
        for created in &seed.pools {
            let (key, pool) = validate_created(created, limits.max_tokens_per_pool)?;
            pools.entry(key).or_insert_with(|| pool.clone());
        }
        let fingerprint = fingerprint(&seed, &pools);
        Ok(Self {
            seed: SeedIdentity {
                chain: seed.chain,
                revision: seed.revision,
                at: seed.at,
                completeness: seed.completeness,
                fingerprint,
            },
            completeness: seed.completeness,
            pools,
            head: seed.at,
            finalized: None,
            history: VecDeque::new(),
            undo_pools: 0,
            started: false,
            limits,
        })
    }

    pub fn seed_identity(&self) -> &SeedIdentity {
        &self.seed
    }
    pub fn completeness(&self) -> Completeness {
        self.completeness
    }
    pub fn head(&self) -> &BlockRef {
        &self.head
    }
    pub fn finalized(&self) -> Option<&BlockRef> {
        self.finalized.as_ref()
    }
    pub fn len(&self) -> usize {
        self.pools.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pools.is_empty()
    }
    pub fn pool(&self, id: &[u8]) -> Option<&Pool> {
        self.pools.get(id)
    }

    /// Prepare all creations before projecting swaps. Parent identity and height
    /// must extend the current head. Failed preparation does not mutate state.
    pub fn prepare_block(&mut self, block: BlockRef, parent_hash: [u8; 32], creations: &Pools) -> Result<PreparedBlock<'_>> {
        check(
            self.head.number.checked_add(1) == Some(block.number) && self.head.hash == parent_hash,
            "registry continuity lost",
        )?;
        let added = self.additions(creations)?;
        check(added.len() <= self.limits.max_undo_pools, "block undo pool bound exceeded")?;
        Ok(PreparedBlock {
            registry: self,
            block,
            added,
            anchor: false,
        })
    }

    /// Startup-only warm anchor. Apply its creations but suppress publication in
    /// the caller. A later height requires explicit acceptance of missing history
    /// and downgrades completeness; no ancestry or missing pools are invented.
    /// The anchor itself becomes the undo floor. Original seed identity is kept.
    pub fn prepare_anchor(&mut self, at: BlockRef, creations: &Pools, allow_partial_history: bool) -> Result<PreparedBlock<'_>> {
        check(!self.started && self.head == self.seed.at, "registry already started")?;
        check(at.number >= self.seed.at.number, "anchor precedes seed")?;
        if at.number == self.seed.at.number {
            check(at.hash == self.seed.at.hash, "anchor conflicts with seed")?;
        } else {
            check(allow_partial_history, "historical seed requires partial-history opt-in")?;
        }
        let added = self.additions(creations)?;
        Ok(PreparedBlock {
            registry: self,
            block: at,
            added,
            anchor: true,
        })
    }

    /// Convenience warm anchor without creations; prefer `prepare_anchor` when
    /// the first observed block is available.
    pub fn anchor_at(&mut self, at: BlockRef, allow_partial_history: bool) -> Result<()> {
        self.prepare_anchor(at, &Pools::default(), allow_partial_history)?.commit();
        Ok(())
    }

    fn additions(&self, creations: &Pools) -> Result<BTreeMap<Vec<u8>, Pool>> {
        check(creations.pools.len() <= self.limits.max_pools_per_block, "block entry bound exceeded")?;
        let mut added = BTreeMap::new();
        for created in &creations.pools {
            let (key, pool) = validate_created(created, self.limits.max_tokens_per_pool)?;
            if !self.pools.contains_key(&key) {
                added.entry(key).or_insert_with(|| pool.clone());
                check(added.len() <= self.limits.max_pools - self.pools.len(), "registry pool bound exceeded")?;
            }
        }
        Ok(added)
    }

    /// Check an exact retained canonical target without mutating state.
    pub fn can_undo(&self, target: &BlockRef) -> Result<()> {
        self.undo_count(target).map(|_| ())
    }

    pub fn prepare_undo(&mut self, target: &BlockRef) -> Result<PreparedUndo<'_>> {
        let frames = self.undo_count(target)?;
        Ok(PreparedUndo { registry: self, frames })
    }

    pub fn undo_to(&mut self, target: &BlockRef) -> Result<()> {
        self.prepare_undo(target)?.commit();
        Ok(())
    }

    fn undo_count(&self, target: &BlockRef) -> Result<usize> {
        check(self.finalized.is_none_or(|at| target.number >= at.number), "cannot undo finalized registry")?;
        if *target == self.head {
            return Ok(0);
        }
        self.history
            .iter()
            .rev()
            .position(|frame| frame.parent == *target)
            .map(|i| i + 1)
            .ok_or(Error("undo outside retained registry history"))
    }

    pub fn can_finalize(&self, target: &BlockRef) -> Result<()> {
        check(self.finalized.is_none_or(|at| target.number >= at.number), "registry finality moved backwards")?;
        check(
            self.finalized == Some(*target) || *target == self.head || self.history.iter().any(|frame| frame.block == *target || frame.parent == *target),
            "finality outside retained registry history",
        )
    }

    pub fn prepare_finalize(&mut self, target: &BlockRef) -> Result<PreparedFinalize<'_>> {
        self.can_finalize(target)?;
        Ok(PreparedFinalize {
            registry: self,
            target: *target,
        })
    }

    pub fn finalize(&mut self, target: &BlockRef) -> Result<()> {
        self.prepare_finalize(target)?.commit();
        Ok(())
    }
}

impl PoolLookup for Registry {
    fn get_pool(&self, address: &[u8]) -> Option<Pool> {
        self.pool(address).cloned()
    }
}

/// Uncommitted complete-block lookup. The exclusive registry borrow prevents
/// stale commits; dropping this value rolls back without copying the full map.
#[derive(Debug)]
pub struct PreparedBlock<'a> {
    registry: &'a mut Registry,
    block: BlockRef,
    added: BTreeMap<Vec<u8>, Pool>,
    anchor: bool,
}

impl PreparedBlock<'_> {
    pub fn block(&self) -> &BlockRef {
        &self.block
    }
    pub fn seed_identity(&self) -> &SeedIdentity {
        self.registry.seed_identity()
    }
    pub fn completeness(&self) -> Completeness {
        if self.anchor && self.block.number > self.registry.seed.at.number {
            Completeness::Partial
        } else {
            self.registry.completeness
        }
    }
    pub fn len(&self) -> usize {
        self.registry.len() + self.added.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn pool(&self, id: &[u8]) -> Option<&Pool> {
        self.registry.pool(id).or_else(|| self.added.get(id))
    }
    /// First-writer insertions only, in canonical key order (not event order).
    pub fn new_pools(&self) -> impl Iterator<Item = (&[u8], &Pool)> {
        self.added.iter().map(|(key, pool)| (key.as_slice(), pool))
    }

    /// Infallible after preparation. Coordinate downstream validation before
    /// this call. Allocation failure remains Rust's ordinary process behavior.
    pub fn commit(self) {
        self.registry.completeness = self.completeness();
        if !self.anchor {
            self.registry.undo_pools += self.added.len();
            self.registry.history.push_back(UndoFrame {
                block: self.block,
                parent: self.registry.head,
                added: self.added.keys().cloned().collect(),
            });
            while self.registry.history.len() > self.registry.limits.max_undo_blocks || self.registry.undo_pools > self.registry.limits.max_undo_pools {
                let frame = self.registry.history.pop_front().expect("nonempty bounded history");
                self.registry.undo_pools -= frame.added.len();
            }
        }
        self.registry.pools.extend(self.added);
        self.registry.head = self.block;
        self.registry.started = true;
    }
}

impl PoolLookup for PreparedBlock<'_> {
    fn get_pool(&self, address: &[u8]) -> Option<Pool> {
        self.pool(address).cloned()
    }
}

/// Validated undo. Dropping it leaves the registry unchanged.
#[derive(Debug)]
pub struct PreparedUndo<'a> {
    registry: &'a mut Registry,
    frames: usize,
}

impl PreparedUndo<'_> {
    pub fn commit(self) {
        for _ in 0..self.frames {
            let frame = self.registry.history.pop_back().expect("validated undo frame");
            self.registry.undo_pools -= frame.added.len();
            for key in frame.added {
                self.registry.pools.remove(&key);
            }
            self.registry.head = frame.parent;
        }
    }
}

/// Validated finality. Dropping it leaves the registry unchanged.
#[derive(Debug)]
pub struct PreparedFinalize<'a> {
    registry: &'a mut Registry,
    target: BlockRef,
}

impl PreparedFinalize<'_> {
    pub fn commit(self) {
        self.registry.finalized = Some(self.target);
        while self.registry.history.front().is_some_and(|frame| frame.block.number <= self.target.number) {
            let frame = self.registry.history.pop_front().expect("checked final history");
            self.registry.undo_pools -= frame.added.len();
        }
    }
}

fn validate_created(created: &PoolCreated, max_tokens: usize) -> Result<(Vec<u8>, &Pool)> {
    check(
        matches!(created.address.len(), 40 | 64) && created.address.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "pool ID must be lowercase unprefixed 20/32-byte hex",
    )?;
    let key = hex::decode(&created.address).map_err(|_| Error("invalid pool ID"))?;
    let pool = created.pool.as_option().ok_or(Error("missing pool metadata"))?;
    check(pool.tokens.len() <= max_tokens, "pool token bound exceeded")?;
    check(pool.tokens.iter().all(|token| token.len() == 20), "invalid token width")?;
    check(pool.factory.is_empty() || pool.factory.len() == 20, "invalid factory width")?;
    Ok((key, pool))
}

fn check(condition: bool, message: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error(message))
    }
}

fn fingerprint(seed: &Seed, pools: &BTreeMap<Vec<u8>, Pool>) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"dex-pool-registry-seed-v1\0");
    hash_bytes(&mut digest, seed.chain.as_bytes());
    hash_bytes(&mut digest, seed.revision.as_bytes());
    digest.update(seed.at.number.to_be_bytes());
    digest.update(seed.at.hash);
    digest.update([match seed.completeness {
        Completeness::Complete => 1,
        Completeness::Partial => 0,
    }]);
    digest.update((pools.len() as u64).to_be_bytes());
    for (key, pool) in pools {
        hash_bytes(&mut digest, key);
        hash_bytes(&mut digest, &pool.factory);
        digest.update((pool.tokens.len() as u64).to_be_bytes());
        for token in &pool.tokens {
            hash_bytes(&mut digest, token);
        }
    }
    digest.finalize().into()
}

fn hash_bytes(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
}

#[cfg(test)]
mod tests;
