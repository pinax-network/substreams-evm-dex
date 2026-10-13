//! The first two tests are v0.8.0's, adapted to buffa; the others feed
//! synthetic pool creations through the per-protocol packages' `project`.

use super::*;
use common::testing::*;
use substreams::buffa::Message;

fn decode_pool(entry: &Entry) -> foundational::Pool {
    let value = entry.value.as_option().unwrap();
    assert_eq!(value.type_url, "type.googleapis.com/dex.foundational_store.v1.Pool");
    foundational::Pool::decode(&mut value.value.as_ref()).unwrap()
}

#[test]
fn stores_single_token_without_factory() {
    let mut entries = Vec::new();
    push_pool_entry(&mut entries, &[0x10], vec![vec![0xaa]], vec![]);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].key.as_option().unwrap().bytes, vec![0x10]);

    let pool = decode_pool(&entries[0]);
    assert_eq!(pool.tokens, vec![vec![0xaa]]);
    assert!(pool.factory.is_empty());
}

#[test]
fn extracts_balancer_tokens_from_token_config() {
    let mut entries = Vec::new();
    push_balancer_pool(
        &mut entries,
        &[0x01],
        &[0x02],
        &[
            balancer::TokenConfig {
                token: vec![0xaa],
                token_type: 0,
                rate_provider: vec![],
                paysYieldFees: false,
            },
            balancer::TokenConfig {
                token: vec![0xbb],
                token_type: 0,
                rate_provider: vec![],
                paysYieldFees: false,
            },
        ],
    );

    let pool = decode_pool(&entries[0]);
    assert_eq!(pool.tokens, vec![vec![0xaa], vec![0xbb]]);
    assert_eq!(pool.factory, vec![0x02]);
}

#[test]
fn a_block_without_pool_creations_gives_the_empty_default() {
    assert_eq!(project(&ProtocolEvents::default()), SinkEntries::default());
}

#[test]
fn a_pool_with_an_empty_key_is_skipped() {
    let mut entries = Vec::new();
    push_pool_entry(&mut entries, &[], vec![vec![0xaa]], vec![0x02]);
    assert!(entries.is_empty());
}

/// An entry's key, tokens and factory.
type Summary = (Vec<u8>, Vec<Vec<u8>>, Vec<u8>);

fn summary(entries: &SinkEntries) -> Vec<Summary> {
    entries
        .entries
        .iter()
        .map(|entry| {
            let pool = decode_pool(entry);
            (entry.key.as_option().unwrap().bytes.clone(), pool.tokens, pool.factory)
        })
        .collect()
}

#[test]
fn collects_the_pool_creations_of_every_package_in_input_order() {
    let factory = addr(0xfa);
    let created = |signature: &str, indexed: Vec<Vec<u8>>, data: Vec<u8>| {
        let mut topics = vec![topic0(signature)];
        topics.extend(indexed);
        log(&factory, topics, data)
    };
    // One transaction per creation; a CurveFi factory deployment CREATEs its
    // pool (0xc0) in the root call.
    let mut curve_deployment = trx(
        &factory,
        vec![created(
            "MetaPoolDeployed(address,address,uint256,uint256,address)",
            vec![],
            [word(&addr(0xc1)), word(&addr(0xc2)), uint(100), uint(4), word(&addr(0xc3))].concat(),
        )],
    );
    curve_deployment.calls[0].call_type = eth::CallType::Create.into();
    curve_deployment.calls[0].address = addr(0xc0);
    let block = block(vec![
        trx(
            &factory,
            vec![created(
                "PairCreated(address,address,address,uint256)",
                vec![word(&addr(0x21)), word(&addr(0x22))],
                [word(&addr(0x20)), uint(1)].concat(),
            )],
        ),
        trx(
            &factory,
            vec![created(
                "PoolCreated(address,address,uint24,int24,address)",
                vec![word(&addr(0x31)), word(&addr(0x32)), uint(500)],
                [int(10), word(&addr(0x30))].concat(),
            )],
        ),
        trx(
            &factory,
            vec![created(
                "TokenCreate(address,uint256,address)",
                vec![],
                [word(&addr(0x50)), uint(1), word(&addr(0x51))].concat(),
            )],
        ),
        curve_deployment,
    ]);

    let events = ProtocolEvents {
        sunpump: ::sunpump::project(&block).unwrap(),
        balancer: ::balancer::project(&block).unwrap(),
        bancor: ::bancor::project(&block).unwrap(),
        curvefi: ::curvefi::project(&block).unwrap(),
        aerodrome: ::aerodrome::project(&block).unwrap(),
        traderjoe: ::traderjoe::project(&block).unwrap(),
        kyber_elastic: ::kyber_elastic::project(&block).unwrap(),
        uniswap_v1: ::uniswap_v1::project(&block).unwrap(),
        uniswap_v2: ::uniswap_v2::project(&block).unwrap(),
        uniswap_v3: ::uniswap_v3::project(&block).unwrap(),
        uniswap_v4: ::uniswap_v4::project(&block).unwrap(),
    };
    let entries = project(&events);

    assert!(entries.if_not_exist);
    assert_eq!(
        summary(&entries),
        vec![
            // SunPump first, keyed by the token.
            (addr(0x50), vec![], factory.clone()),
            (addr(0xc0), vec![addr(0xc1)], factory.clone()),
            // Kyber Elastic shares Uniswap V3's PoolCreated, so both list it.
            (addr(0x30), vec![addr(0x31), addr(0x32)], factory.clone()),
            (addr(0x20), vec![addr(0x21), addr(0x22)], factory.clone()),
            (addr(0x30), vec![addr(0x31), addr(0x32)], factory.clone()),
        ]
    );
}
