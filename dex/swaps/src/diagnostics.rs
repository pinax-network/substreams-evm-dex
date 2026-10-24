//! Observations from the existing decoder branches, without changing their output.

use crate::{logs::PoolMetadataMap, pb, Pool};

/// Counts of ABI-matched swap candidates for one protocol interpretation.
///
/// These are not counts of authenticated pools, unique chain executions, or
/// swaps guaranteed to emit once metadata becomes available. Amount/direction
/// checks still run after the metadata checks. V3 and Kyber count separately.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CandidateCounts {
    /// Successfully ABI-decoded swap events, including CoW, DODO and WOOFi.
    /// Fee and initialization events are excluded.
    pub matched_swap_candidates: u64,
    /// Candidates whose required pool was absent from the preloaded block map.
    /// This does not prove absence from the caller's registry: preloading uses
    /// receipt logs while decoding can use call logs.
    pub missing_pool_metadata: u64,
    /// Candidates with a known pool but an absent required token position, or
    /// an invalid Curve token index. Counted once per candidate.
    pub missing_token_context: u64,
    /// Candidates with all required token positions present but a selected
    /// token whose width is not 20 bytes. Counted once, without rejecting or
    /// changing the legacy output. Zero addresses and repeated tokens are valid.
    pub malformed_token_context: u64,
}

impl CandidateCounts {
    pub(crate) fn matched(&mut self) {
        self.matched_swap_candidates = self.matched_swap_candidates.saturating_add(1);
    }

    pub(crate) fn missing_token(&mut self) {
        self.missing_token_context = self.missing_token_context.saturating_add(1);
    }

    pub(crate) fn observe_tokens(&mut self, tokens: &[&[u8]]) {
        if tokens.iter().any(|token| token.len() != 20) {
            self.malformed_token_context = self.malformed_token_context.saturating_add(1);
        }
    }

    fn add(&mut self, other: Self) {
        self.matched_swap_candidates = self.matched_swap_candidates.saturating_add(other.matched_swap_candidates);
        self.missing_pool_metadata = self.missing_pool_metadata.saturating_add(other.missing_pool_metadata);
        self.missing_token_context = self.missing_token_context.saturating_add(other.missing_token_context);
        self.malformed_token_context = self.malformed_token_context.saturating_add(other.malformed_token_context);
    }
}

/// Fixed-size per-protocol observations from one successful block projection.
///
/// No addresses, logs, unbounded collections, or new pool lookups are retained.
/// Unknown or unsupported signatures do not contribute to these counters; they
/// cannot establish overall protocol coverage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SwapDiagnostics {
    counts: [CandidateCounts; 15],
}

impl SwapDiagnostics {
    /// Counts for a known protocol; unspecified has zero candidates.
    pub fn for_protocol(&self, protocol: pb::Protocol) -> CandidateCounts {
        self.counts[protocol as usize]
    }

    /// Sum of protocol interpretations, including V3 and Kyber separately.
    pub fn total(&self) -> CandidateCounts {
        let mut total = CandidateCounts::default();
        for counts in self.counts {
            total.add(counts);
        }
        total
    }

    pub(crate) fn for_protocol_mut(&mut self, protocol: pb::Protocol) -> &mut CandidateCounts {
        &mut self.counts[protocol as usize]
    }
}

/// Called only after a protocol's existing `match_and_decode` succeeds.
pub(crate) fn swap_pool<'a>(pools: &'a PoolMetadataMap, key: &[u8], counts: &mut CandidateCounts) -> Option<&'a Pool> {
    counts.matched();
    let pool = pools.get(key);
    if pool.is_none() {
        counts.missing_pool_metadata = counts.missing_pool_metadata.saturating_add(1);
    }
    pool
}
