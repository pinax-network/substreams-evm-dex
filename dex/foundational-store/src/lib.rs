//! Foundational-store entries for DEX pools: a pool's tokens and factory,
//! keyed by its address (Uniswap V4: the pool id; SunPump: the token), from
//! the pool creations in the `map_events` outputs of eleven per-protocol
//! packages.
//!
//! Ported from substreams-evm `dex-swaps-pools-foundational-store` (crate
//! `evm-dex-foundational-store`, tag `evm-dex-v0.8.0`, `9663b25`); the
//! package README lists what changed. [`project`] builds the entries without
//! the Substreams runtime, for a host program that runs the per-protocol
//! packages' `project` itself; the `map_entries` handler exists only in the
//! WASM build.
//!
//! The foundational payload is intentionally restricted to shared pool
//! metadata only: tokens[] plus factory when available. Protocol-specific
//! initialization fields like Aerodrome `stable`, TraderJoe `bin_step`, and
//! Kyber `swap_fee_units` / `tick_distance` remain available on the original
//! pool-creation events.

use proto::pb::dex::foundational_store::v1 as foundational;
use proto::pb::{
    aerodrome::v1 as aerodrome, balancer::v1 as balancer, bancor::v1 as bancor, curvefi::v1 as curvefi, kyber_elastic::v1 as kyber_elastic,
    sunpump::v1 as sunpump, traderjoe::v1 as traderjoe, uniswap,
};
use substreams::buffa_types::google::protobuf::Any;
/// The foundational-store messages [`project`] returns.
pub use substreams::pb::sf::substreams::foundational_store::model::v2::{Entry, Key, SinkEntries};

/// The type URL of each entry's value, a `dex.foundational_store.v1.Pool`.
pub const POOL_TYPE_URL: &str = "type.googleapis.com/dex.foundational_store.v1.Pool";

/// One block's `map_events` outputs of the per-protocol packages that
/// `map_entries` reads, one field per imported package.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProtocolEvents {
    pub sunpump: sunpump::Events,
    pub balancer: balancer::Events,
    pub bancor: bancor::Events,
    pub curvefi: curvefi::Events,
    pub aerodrome: aerodrome::Events,
    pub traderjoe: traderjoe::Events,
    pub kyber_elastic: kyber_elastic::Events,
    pub uniswap_v1: uniswap::v1::Events,
    pub uniswap_v2: uniswap::v2::Events,
    pub uniswap_v3: uniswap::v3::Events,
    pub uniswap_v4: uniswap::v4::Events,
}

/// The entries `map_entries` emits for one block: a `Pool` per pool
/// creation, in the order of the fields of [`ProtocolEvents`] and then of the
/// events, with `if_not_exist` set (the first writer of a key wins); an empty
/// default when the block created no pool. A pool with an empty key is
/// skipped.
pub fn project(events: &ProtocolEvents) -> SinkEntries {
    let mut entries = Vec::new();

    collect_sunpump(&mut entries, &events.sunpump);
    collect_balancer(&mut entries, &events.balancer);
    collect_bancor(&mut entries, &events.bancor);
    collect_curvefi(&mut entries, &events.curvefi);
    collect_aerodrome(&mut entries, &events.aerodrome);
    collect_traderjoe(&mut entries, &events.traderjoe);
    collect_kyber_elastic(&mut entries, &events.kyber_elastic);
    collect_uniswap_v1(&mut entries, &events.uniswap_v1);
    collect_uniswap_v2(&mut entries, &events.uniswap_v2);
    collect_uniswap_v3(&mut entries, &events.uniswap_v3);
    collect_uniswap_v4(&mut entries, &events.uniswap_v4);

    if entries.is_empty() {
        return SinkEntries::default();
    }

    SinkEntries { entries, if_not_exist: true }
}

fn push_pool_entry(entries: &mut Vec<Entry>, pool: &[u8], tokens: Vec<Vec<u8>>, factory: Vec<u8>) {
    if pool.is_empty() {
        return;
    }

    let value = Any::pack(&foundational::Pool { tokens, factory }, POOL_TYPE_URL);

    entries.push(Entry {
        key: Key { bytes: pool.to_vec() }.into(),
        value: value.into(),
    });
}

fn push_balancer_pool(entries: &mut Vec<Entry>, pool: &[u8], factory: &[u8], token_config: &[balancer::TokenConfig]) {
    push_pool_entry(
        entries,
        pool,
        token_config.iter().map(|config| config.token.clone()).collect(),
        factory.to_vec(),
    );
}

fn collect_aerodrome(entries: &mut Vec<Entry>, events: &aerodrome::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            if let Some(aerodrome::log::Log::PoolCreated(pool_created)) = &log.log {
                push_pool_entry(
                    entries,
                    &pool_created.pool,
                    vec![pool_created.token0.clone(), pool_created.token1.clone()],
                    log.address.clone(),
                );
            }
        }
    }
}

fn collect_balancer(entries: &mut Vec<Entry>, events: &balancer::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            if let Some(balancer::log::Log::PoolRegistered(pool_registered)) = &log.log {
                push_balancer_pool(entries, &pool_registered.pool, &pool_registered.factory, &pool_registered.token_config);
            }
        }
    }
}

fn collect_bancor(entries: &mut Vec<Entry>, events: &bancor::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            match &log.log {
                Some(bancor::log::Log::FeaturesAddition(event)) => {
                    push_pool_entry(entries, &event.address, vec![], log.address.clone());
                }
                Some(bancor::log::Log::NewConverter(event)) => {
                    push_pool_entry(entries, &event.converter, vec![], log.address.clone());
                }
                _ => {}
            }
        }
    }
}

fn collect_curvefi(entries: &mut Vec<Entry>, events: &curvefi::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            match &log.log {
                Some(curvefi::log::Log::Init(init)) => {
                    // CurveFi exposes pool constituents as `coins`; foundational consumers read them
                    // through the shared `tokens[]` field in the normalized payload.
                    push_pool_entry(entries, &init.address, init.coins.clone(), vec![]);
                }
                Some(curvefi::log::Log::PlainPoolDeployed(event)) => {
                    push_pool_entry(entries, &event.address, event.coins.clone(), log.address.clone());
                }
                Some(curvefi::log::Log::MetaPoolDeployed(event)) => {
                    push_pool_entry(entries, &event.address, vec![event.coin.clone()], log.address.clone());
                }
                Some(curvefi::log::Log::CryptoPoolDeployed(event)) => {
                    push_pool_entry(entries, &event.address, event.coins.clone(), log.address.clone());
                }
                _ => {}
            }
        }
    }
}

fn collect_kyber_elastic(entries: &mut Vec<Entry>, events: &kyber_elastic::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            if let Some(kyber_elastic::log::Log::PoolCreated(pool_created)) = &log.log {
                push_pool_entry(
                    entries,
                    &pool_created.pool,
                    vec![pool_created.token0.clone(), pool_created.token1.clone()],
                    log.address.clone(),
                );
            }
        }
    }
}

fn collect_sunpump(entries: &mut Vec<Entry>, events: &sunpump::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            match &log.log {
                Some(sunpump::log::Log::TokenCreate(event)) => {
                    push_pool_entry(entries, &event.token_address, vec![], log.address.clone());
                }
                Some(sunpump::log::Log::TokenCreateLegacy(event)) => {
                    push_pool_entry(entries, &event.token_address, vec![], log.address.clone());
                }
                _ => {}
            }
        }
    }
}

fn collect_traderjoe(entries: &mut Vec<Entry>, events: &traderjoe::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            if let Some(traderjoe::log::Log::LbPairCreated(pair_created)) = &log.log {
                push_pool_entry(
                    entries,
                    &pair_created.lb_pair,
                    vec![pair_created.token_x.clone(), pair_created.token_y.clone()],
                    log.address.clone(),
                );
            }
        }
    }
}

fn collect_uniswap_v1(entries: &mut Vec<Entry>, events: &uniswap::v1::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            if let Some(uniswap::v1::log::Log::NewExchange(new_exchange)) = &log.log {
                push_pool_entry(entries, &new_exchange.exchange, vec![new_exchange.token.clone()], log.address.clone());
            }
        }
    }
}

fn collect_uniswap_v2(entries: &mut Vec<Entry>, events: &uniswap::v2::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            if let Some(uniswap::v2::log::Log::PairCreated(pair_created)) = &log.log {
                push_pool_entry(
                    entries,
                    &pair_created.pair,
                    vec![pair_created.token0.clone(), pair_created.token1.clone()],
                    log.address.clone(),
                );
            }
        }
    }
}

fn collect_uniswap_v3(entries: &mut Vec<Entry>, events: &uniswap::v3::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            if let Some(uniswap::v3::log::Log::PoolCreated(pool_created)) = &log.log {
                push_pool_entry(
                    entries,
                    &pool_created.pool,
                    vec![pool_created.token0.clone(), pool_created.token1.clone()],
                    log.address.clone(),
                );
            }
        }
    }
}

fn collect_uniswap_v4(entries: &mut Vec<Entry>, events: &uniswap::v4::Events) {
    for trx in &events.transactions {
        for log in &trx.logs {
            if let Some(uniswap::v4::log::Log::Initialize(initialize)) = &log.log {
                push_pool_entry(
                    entries,
                    &initialize.id,
                    vec![initialize.currency0.clone(), initialize.currency1.clone()],
                    log.address.clone(),
                );
            }
        }
    }
}

// Only the WASM build exports the package's map_entries.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
mod handler {
    use super::*;

    #[substreams::handlers::map]
    fn map_entries(
        events_sunpump: sunpump::Events,
        events_balancer: balancer::Events,
        events_bancor: bancor::Events,
        events_curvefi: curvefi::Events,
        events_aerodrome: aerodrome::Events,
        events_traderjoe: traderjoe::Events,
        events_kyber_elastic: kyber_elastic::Events,
        events_uniswap_v1: uniswap::v1::Events,
        events_uniswap_v2: uniswap::v2::Events,
        events_uniswap_v3: uniswap::v3::Events,
        events_uniswap_v4: uniswap::v4::Events,
    ) -> Result<SinkEntries, substreams::errors::Error> {
        Ok(project(&ProtocolEvents {
            sunpump: events_sunpump,
            balancer: events_balancer,
            bancor: events_bancor,
            curvefi: events_curvefi,
            aerodrome: events_aerodrome,
            traderjoe: events_traderjoe,
            kyber_elastic: events_kyber_elastic,
            uniswap_v1: events_uniswap_v1,
            uniswap_v2: events_uniswap_v2,
            uniswap_v3: events_uniswap_v3,
            uniswap_v4: events_uniswap_v4,
        }))
    }
}

#[cfg(test)]
mod tests;
