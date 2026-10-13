use proto::pb::dex::swaps::v1 as pb;
use substreams_abis::dex::curvefi as abi;
use substreams_ethereum::{
    pb::eth::v2::{Log, TransactionTrace},
    Event,
};

use crate::logs::PoolMetadataMap;
use crate::utils::truncated_fee;
use substreams::errors::Error;

pub(crate) fn decode_log(tx: &TransactionTrace, log: &Log, pools: &PoolMetadataMap) -> Result<Vec<pb::log::Log>, Error> {
    if let Some(event) = abi::pool::events::TokenExchange::match_and_decode(log) {
        let Some(pool) = pools.get(log.address.as_slice()) else {
            return Ok(Vec::new());
        };
        let Some(input_token) = parse_curve_token(&pool.tokens, &event.sold_id.to_string()) else {
            return Ok(Vec::new());
        };
        let Some(output_token) = parse_curve_token(&pool.tokens, &event.bought_id.to_string()) else {
            return Ok(Vec::new());
        };

        return Ok(vec![pb::log::Log::from(pb::Swap {
            protocol: pb::Protocol::Curvefi.into(),
            factory: pool.factory.clone(),
            pool: log.address.clone(),
            user: event.buyer.to_vec(),
            input_token,
            input_amount: event.tokens_sold.to_string(),
            output_token,
            output_amount: event.tokens_bought.to_string(),
        })]);
    }

    if let Some(event) = abi::cryptoswap::events::TokenExchange::match_and_decode(log) {
        let Some(pool) = pools.get(log.address.as_slice()) else {
            return Ok(Vec::new());
        };
        let Some(input_token) = parse_curve_token(&pool.tokens, &event.sold_id.to_string()) else {
            return Ok(Vec::new());
        };
        let Some(output_token) = parse_curve_token(&pool.tokens, &event.bought_id.to_string()) else {
            return Ok(Vec::new());
        };

        return Ok(vec![pb::log::Log::from(pb::Swap {
            protocol: pb::Protocol::Curvefi.into(),
            factory: pool.factory.clone(),
            pool: log.address.clone(),
            user: event.buyer.to_vec(),
            input_token,
            input_amount: event.tokens_sold.to_string(),
            output_token,
            output_amount: event.tokens_bought.to_string(),
        })]);
    }

    if let Some(event) = abi::pool::events::CommitNewFee::match_and_decode(log) {
        let Some(pool) = pools.get(log.address.as_slice()) else {
            return Ok(Vec::new());
        };

        return Ok(vec![pb::log::Log::from(pb::SwapFee {
            protocol: pb::Protocol::Curvefi.into(),
            factory: pool.factory.clone(),
            pool: log.address.clone(),
            fee: truncated_fee(&event.fee, "CommitNewFee")?,
        })]);
    }

    if let Some(event) = abi::pool::events::NewFee::match_and_decode(log) {
        let Some(pool) = pools.get(log.address.as_slice()) else {
            return Ok(Vec::new());
        };

        return Ok(vec![pb::log::Log::from(pb::SwapFee {
            protocol: pb::Protocol::Curvefi.into(),
            factory: pool.factory.clone(),
            pool: log.address.clone(),
            fee: truncated_fee(&event.fee, "NewFee")?,
        })]);
    }

    if let Some(event) = abi::factory::events::PlainPoolDeployed::match_and_decode(log) {
        let pool = get_create_address(tx).unwrap_or_default();
        return Ok(vec![
            pb::log::Log::from(pb::Initialize {
                protocol: pb::Protocol::Curvefi.into(),
                factory: log.address.clone(),
                pool: pool.clone(),
            }),
            pb::log::Log::from(pb::SwapFee {
                protocol: pb::Protocol::Curvefi.into(),
                factory: log.address.clone(),
                pool,
                fee: truncated_fee(&event.fee, "PlainPoolDeployed")?,
            }),
        ]);
    }

    if let Some(event) = abi::factory::events::MetaPoolDeployed::match_and_decode(log) {
        let pool = get_create_address(tx).unwrap_or_default();
        return Ok(vec![
            pb::log::Log::from(pb::Initialize {
                protocol: pb::Protocol::Curvefi.into(),
                factory: log.address.clone(),
                pool: pool.clone(),
            }),
            pb::log::Log::from(pb::SwapFee {
                protocol: pb::Protocol::Curvefi.into(),
                factory: log.address.clone(),
                pool,
                fee: truncated_fee(&event.fee, "MetaPoolDeployed")?,
            }),
        ]);
    }

    Ok(Vec::new())
}

fn parse_curve_token(tokens: &[Vec<u8>], index: &str) -> Option<Vec<u8>> {
    let index = index.parse::<usize>().ok()?;
    tokens.get(index).cloned()
}

fn get_create_address(tx: &TransactionTrace) -> Option<Vec<u8>> {
    tx.calls
        .iter()
        .find(|call| !call.state_reverted && !call.address.is_empty())
        .map(|call| call.address.clone())
}
