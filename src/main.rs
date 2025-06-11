//#![allow(async_fn_in_trait, unused_variables)]

use std::{
    net::ToSocketAddrs,
    sync::{
        Mutex,
        mpsc::{Receiver, channel},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, bail};
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    select,
    time::{Instant, sleep_until},
};

use pprof::protos::Message;
use types::{addressportnetwork::AddressPortNetwork, network_id::NetworkId};

pub mod addrman;
pub mod connect;
pub mod crawler;
pub mod db;
pub mod packets;
pub mod types;

const DNS_SEEDS: &[&'static str] = &[
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

#[tokio::main]
async fn main() -> Result<()> {
    let ctrl_c_events = ctrl_channel()?;

    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .unwrap();
    db::setup().await;
    addrman::start().await;
    tokio::spawn(resolve_dns_and_add_to_addrman(DNS_SEEDS));
    let c = tokio::spawn(crawler::crawl_forever());

    ctrl_c_events.recv().unwrap();

    println!("received a ctrl+c");

    c.abort();

    let report = guard.report().build()?;
    let mut file = File::create("profile.pb").await.unwrap();
    let profile = report.pprof().unwrap();

    let mut content = Vec::new();
    profile.write_to_vec(&mut content).unwrap();

    file.write_all(&content).await.unwrap();

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
            Err(e) => println!("failed to resolve {}: {}", addy, e),
        }
    }
    resolved_peers.sort();
    resolved_peers.dedup();
    println!("resolved {} seed peers", resolved_peers.len());
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
