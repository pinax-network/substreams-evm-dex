use proto::pb::dex::swaps::v1 as pb;
use substreams_abis::dex::sunpump as abi;
use substreams_abis::dex::sunpump::legacy::launchpad::events::TokenCreate as TokenCreateLegacy;
use substreams_ethereum::{
    pb::eth::v2::{Log, TransactionTrace},
    Event, NULL_ADDRESS,
};

use crate::diagnostics::{swap_pool, CandidateCounts};
use crate::logs::PoolMetadataMap;
use crate::utils::truncated_fee;
use substreams::errors::Error;

pub(crate) fn decode_log(_tx: &TransactionTrace, log: &Log, pools: &PoolMetadataMap, counts: &mut CandidateCounts) -> Result<Vec<pb::log::Log>, Error> {
    if let Some(event) = abi::v1::launchpadproxy::events::TokenPurchased::match_and_decode(log) {
        let Some(pool) = swap_pool(pools, event.token.as_slice(), counts) else {
            return Ok(Vec::new());
        };

        return Ok(vec![pb::log::Log::from(pb::Swap {
            protocol: pb::Protocol::Sunpump.into(),
            factory: pool.factory.clone(),
            pool: event.token.to_vec(),
            user: event.buyer.to_vec(),
            input_token: NULL_ADDRESS.to_vec(),
            input_amount: event.trx_amount.to_string(),
            output_token: event.token.to_vec(),
            output_amount: event.token_amount.to_string(),
        })]);
    }

    if let Some(event) = abi::v1::launchpadproxy::events::TokenSold::match_and_decode(log) {
        let Some(pool) = swap_pool(pools, event.token.as_slice(), counts) else {
            return Ok(Vec::new());
        };

        return Ok(vec![pb::log::Log::from(pb::Swap {
            protocol: pb::Protocol::Sunpump.into(),
            factory: pool.factory.clone(),
            pool: event.token.to_vec(),
            user: event.seller.to_vec(),
            input_token: event.token.to_vec(),
            input_amount: event.token_amount.to_string(),
            output_token: NULL_ADDRESS.to_vec(),
            output_amount: event.trx_amount.to_string(),
        })]);
    }

    if let Some(event) = abi::v1::launchpadproxy::events::TokenCreate::match_and_decode(log) {
        return Ok(vec![pb::log::Log::from(pb::Initialize {
            protocol: pb::Protocol::Sunpump.into(),
            factory: log.address.clone(),
            pool: event.token_address.to_vec(),
        })]);
    }

    if let Some(event) = TokenCreateLegacy::match_and_decode(log) {
        return Ok(vec![pb::log::Log::from(pb::Initialize {
            protocol: pb::Protocol::Sunpump.into(),
            factory: log.address.clone(),
            pool: event.token_address.to_vec(),
        })]);
    }

    if let Some(event) = abi::v1::launchpadproxy::events::PurchaseFeeSet::match_and_decode(log) {
        return Ok(vec![pb::log::Log::from(pb::SwapFee {
            protocol: pb::Protocol::Sunpump.into(),
            factory: log.address.clone(),
            pool: log.address.clone(),
            fee: truncated_fee(&event.new_fee, "PurchaseFeeSet")?,
        })]);
    }

    if let Some(event) = abi::v1::launchpadproxy::events::SaleFeeSet::match_and_decode(log) {
        return Ok(vec![pb::log::Log::from(pb::SwapFee {
            protocol: pb::Protocol::Sunpump.into(),
            factory: log.address.clone(),
            pool: log.address.clone(),
            fee: truncated_fee(&event.new_fee, "SaleFeeSet")?,
        })]);
    }

    Ok(Vec::new())
}
