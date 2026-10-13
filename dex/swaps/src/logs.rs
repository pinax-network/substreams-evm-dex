use std::collections::{HashMap, HashSet};

use substreams_abis::dex::balancer;
use substreams_abis::dex::sunpump;
use substreams_abis::dex::uniswap::v4 as uniswap_v4;
use substreams_ethereum::{
    pb::eth::v2::{Block, Log},
    Event,
};

use crate::{Pool, PoolLookup};

pub(crate) type PoolMetadataMap = HashMap<Vec<u8>, Pool>;

/// The keys looked up for a block: every emitting address of a successful
/// transaction's receipt logs, plus the pool ids of Uniswap V4 and Balancer
/// V3 swaps and the tokens of SunPump trades. Other pools named only in an
/// event (a V4 `Initialize`, a Balancer `AggregateSwapFeePercentageChanged`)
/// are found only if one of these also names them, as in v0.8.0.
pub(crate) fn collect_log_addresses(block: &Block) -> HashSet<Vec<u8>> {
    let mut addresses = HashSet::new();

    for log in block.logs() {
        collect_log_address(log.log, &mut addresses);
    }

    addresses
}

fn collect_log_address(log: &Log, addresses: &mut HashSet<Vec<u8>>) {
    if !log.address.is_empty() {
        addresses.insert(log.address.clone());
    }

    if let Some(event) = uniswap_v4::poolmanager::events::Swap::match_and_decode(log) {
        addresses.insert(event.id.to_vec());
    }

    if let Some(event) = balancer::v3::vault::events::Swap::match_and_decode(log) {
        addresses.insert(event.pool.to_vec());
    }

    if let Some(event) = sunpump::v1::launchpadproxy::events::TokenPurchased::match_and_decode(log) {
        addresses.insert(event.token.to_vec());
    }

    if let Some(event) = sunpump::v1::launchpadproxy::events::TokenSold::match_and_decode(log) {
        addresses.insert(event.token.to_vec());
    }
}

/// Look up pool metadata for each key once, as v0.8.0 read the legacy
/// `store_pools` key-value store.
pub(crate) fn get_pools_by_address<P: PoolLookup + ?Sized>(pools: &P, addresses: &HashSet<Vec<u8>>) -> PoolMetadataMap {
    addresses
        .iter()
        .filter_map(|address| pools.get_pool(address).map(|pool| (address.clone(), pool)))
        .collect()
}

#[cfg(test)]
mod tests {
    use substreams::Hex;

    #[test]
    fn key_matches_store_pools_encoding() {
        // `store_pools` keys entries via `Hex::encode(address)` — lowercase, no `0x`.
        assert_eq!(Hex::encode([0xde, 0xad, 0xbe, 0xef]), "deadbeef");
    }
}
