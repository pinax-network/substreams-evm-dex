use proto::pb::dex::swaps::v1 as pb;
use substreams_abis::dex::bancor::bancorconverterfactory;
use substreams_abis::dex::bancor::converterfactory;
use substreams_abis::dex::bancor::standardpoolconverter as abi;
use substreams_ethereum::{
    pb::eth::v2::{Log, TransactionTrace},
    Event,
};

use crate::logs::PoolMetadataMap;
use crate::utils::truncated_fee;
use substreams::errors::Error;

pub(crate) fn decode_log(_tx: &TransactionTrace, log: &Log, pools: &PoolMetadataMap) -> Result<Vec<pb::log::Log>, Error> {
    if let Some(event) = abi::events::Conversion::match_and_decode(log) {
        let Some(pool) = pools.get(log.address.as_slice()) else {
            return Ok(Vec::new());
        };

        return Ok(vec![pb::log::Log::from(pb::Swap {
            protocol: pb::Protocol::Bancor.into(),
            factory: pool.factory.clone(),
            pool: log.address.clone(),
            user: event.trader.to_vec(),
            input_token: event.source_token.to_vec(),
            input_amount: event.source_amount.to_string(),
            output_token: event.target_token.to_vec(),
            output_amount: event.target_amount.to_string(),
        })]);
    }

    if let Some(event) = abi::events::ConversionFeeUpdate::match_and_decode(log) {
        let Some(pool) = pools.get(log.address.as_slice()) else {
            return Ok(Vec::new());
        };

        return Ok(vec![pb::log::Log::from(pb::SwapFee {
            protocol: pb::Protocol::Bancor.into(),
            factory: pool.factory.clone(),
            pool: log.address.clone(),
            fee: truncated_fee(&event.new_fee, "ConversionFeeUpdate")?,
        })]);
    }

    if let Some(event) = converterfactory::events::NewConverter::match_and_decode(log) {
        return Ok(vec![pb::log::Log::from(pb::Initialize {
            protocol: pb::Protocol::Bancor.into(),
            factory: log.address.clone(),
            pool: event.converter.to_vec(),
        })]);
    }

    if let Some(event) = bancorconverterfactory::events::NewConverter::match_and_decode(log) {
        return Ok(vec![pb::log::Log::from(pb::Initialize {
            protocol: pb::Protocol::Bancor.into(),
            factory: log.address.clone(),
            pool: event.converter.to_vec(),
        })]);
    }

    Ok(Vec::new())
}
