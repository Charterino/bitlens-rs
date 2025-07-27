//#![allow(async_fn_in_trait, unused_variables)]

use std::{
    net::ToSocketAddrs,
    sync::mpsc::{Receiver, channel},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, bail};
use deadpool::unmanaged::Object;
use log::setup_logging;
use packets::{
    network_id::NetworkId,
    packet::{DESERIALIZE_POOL, SERIALIZE_POOL},
};
use slog_scope::{debug, info};
use tokio::{
    runtime::Runtime,
    select,
    time::{Instant, sleep_until},
};

use types::addressportnetwork::AddressPortNetwork;

pub mod addrman;
pub mod api;
pub mod chainman;
pub mod connect;
pub mod crawler;
pub mod db;
pub mod log;
pub mod metrics;
pub mod packets;
pub mod tx;
pub mod types;
pub mod util;

#[cfg(all(not(test), not(target_env = "msvc")))]
use tikv_jemallocator::Jemalloc;

#[cfg(all(not(test), not(target_env = "msvc")))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

const DNS_SEEDS: &[&str] = &[
    "seed.bitcoin.sipa.be",
    "dnsseed.bluematt.me",
    "dnsseed.bitcoin.dashjr-list-of-p2p-nodes.us",
    "seed.bitcoin.jonasschnelli.ch",
    "seed.btc.petertodd.net",
    "seed.bitcoin.sprovoost.nl",
    "dnsseed.emzy.de",
    "seed.bitcoin.wiz.biz",
    "seed.mainnet.achownodes.xyz",
];

fn main() -> Result<()> {
    let _guard = setup_logging();
    let runtime: Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _ = runtime.block_on(async_main());
    drop(runtime);
    drop(_guard);
    Ok(())
}

async fn async_main() -> Result<()> {
    let ctrl_c_events = ctrl_channel()?;

    info!("starting bitlens..");
    metrics::start().await;

    db::setup().await;
    addrman::start().await;
    chainman::start().await;
    api::start().await;
    tokio::spawn(resolve_dns_and_add_to_addrman(DNS_SEEDS));
    let c = tokio::spawn(crawler::crawl_forever());

    ctrl_c_events.recv().unwrap();

    info!("stopping bitlens..");

    c.abort();

    Ok(())
}

async fn resolve_dns_and_add_to_addrman(peers: &[&'static str]) {
    let mut resolved_peers = Vec::new();
    let mut handles = Vec::with_capacity(peers.len());
    for peer in peers {
        handles.push(tokio::spawn(resolve_dns_with_timeout(
            peer,
            Duration::from_secs(1),
        )));
    }
    for i in 0..handles.len() {
        let handle = handles.pop().unwrap();
        let addy = peers[peers.len() - i - 1];
        match handle.await.unwrap() {
            Ok(addys) => resolved_peers.extend_from_slice(&addys),
            Err(e) => debug!("failed to resolve dns"; "address" => addy, "err" => e.to_string()),
        }
    }
    resolved_peers.sort();
    resolved_peers.dedup();
    info!("resolved seed peers"; "count" => resolved_peers.len());
    addrman::peers_seen(
        resolved_peers,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .await;
}

async fn resolve_dns_with_timeout(
    peer: &str,
    timeout: Duration,
) -> Result<Vec<AddressPortNetwork>> {
    let deadline = Instant::now().checked_add(timeout).unwrap();
    let cloned_address = peer.to_string();
    let resolve = tokio::spawn(async move { resolve_dns(cloned_address) });
    select! {
        _ = sleep_until(deadline) => {
            bail!("deadline reached")
        }
        resolved = resolve => {
            resolved.unwrap()
        }
    }
}

fn resolve_dns(peer: String) -> Result<Vec<AddressPortNetwork>> {
    match (peer, 80).to_socket_addrs() {
        Ok(addresses) => Ok(addresses
            .map(|x| AddressPortNetwork {
                network_id: if x.is_ipv4() {
                    NetworkId::IPv4
                } else {
                    NetworkId::IPv6
                },
                port: 8333,
                address: match x.ip().to_canonical() {
                    std::net::IpAddr::V4(ipv4_addr) => ipv4_addr.octets().into(),
                    std::net::IpAddr::V6(ipv6_addr) => ipv6_addr.octets().into(),
                },
            })
            .collect()),
        Err(e) => bail!(e),
    }
}

fn ctrl_channel() -> Result<Receiver<()>, ctrlc::Error> {
    let (sender, receiver) = channel();
    ctrlc::set_handler(move || {
        sender.send(()).unwrap();
    })?;

    Ok(receiver)
}
