use std::{
    borrow::Cow,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Error, Result, bail};
use slog_scope::info;
use tokio::time::{self, Instant, sleep};

use crate::{
    addrman::{self, peers_seen},
    connect::connect_and_handshake,
    packets::{getaddr::GetAddr, network_id::NetworkId, packetpayload::PacketPayloadType},
    types::addressportnetwork::AddressPortNetwork,
};

const NO_PEERS_SLEEP_DURATION: Duration = Duration::from_secs(1);
const SLEEP_BETWEEN_CRAWLS_DURATION: Duration = Duration::from_millis(20);

pub async fn crawl_forever() {
    loop {
        let peers_to_check = addrman::get_peers_to_check();
        if peers_to_check.is_empty() {
            time::sleep(NO_PEERS_SLEEP_DURATION).await;
            continue;
        }

        info!("will scrape peers"; "count" => peers_to_check.len());

        for peer in peers_to_check {
            time::sleep(SLEEP_BETWEEN_CRAWLS_DURATION).await;
            tokio::spawn(crawl_with_backoff(peer));
        }
    }
}

#[derive(Default)]
struct CrawlResult {
    should_insta_ban: bool,
    connect_handshake_error: Option<Error>,
    packets_received_total: u32,
    connection_time: Duration,
}

async fn crawl_with_backoff(peer: AddressPortNetwork) {
    let mut failed_times = 0;
    let mut fail_reasons = Vec::with_capacity(14);
    let mut crawl_result = CrawlResult::default();

    while failed_times < 14 {
        sleep(backoff_from_failed(failed_times)).await;
        let start = Instant::now();
        let disconnect_reason = crawl(&peer, &mut crawl_result).await.unwrap_err();
        crawl_result.connection_time = Instant::now().duration_since(start);
        if let Some(reason) =
            evaluate_crawl_result(&crawl_result, disconnect_reason, &mut failed_times)
        {
            fail_reasons.push(reason);
        } else {
            fail_reasons.clear();
        }
    }
}

fn backoff_from_failed(failed_times: u32) -> Duration {
    if failed_times == 0 {
        return Duration::from_secs(0);
    } else if failed_times <= 12 {
        return Duration::from_secs(2u64.pow(failed_times));
    }
    Duration::from_secs(3600 * 2)
}

fn evaluate_crawl_result(
    result: &CrawlResult,
    disconnect_reason: Error,
    failed_times: &mut u32,
) -> Option<String> {
    if result.connect_handshake_error.is_some() {
        *failed_times += 1;
        return Some(format!(
            "connect/handshake failed: {}",
            result.connect_handshake_error.as_ref().unwrap()
        ));
    }

    if result.should_insta_ban {
        *failed_times = 14;
        return Some(format!("insta_ban requested: {}", disconnect_reason));
    }

    if result.packets_received_total < 10 {
        // Some peers will accept connections, handshake, and disconnect.
        return Some(format!(
            "packet count below minumum threshold: wanted 10 got {}",
            result.packets_received_total
        ));
    }

    if result.connection_time.as_secs() < 10 {
        // Some peers will accept connections, handshake, and disconnect.
        return Some(format!(
            "connection duration below minumum threshold: wanted >=10 seconds got {} seconds",
            result.connection_time.as_secs()
        ));
    }

    *failed_times = 0;
    None
}

async fn crawl(peer: &AddressPortNetwork, res: &mut CrawlResult) -> Result<()> {
    res.packets_received_total = 0;
    match peer.network_id {
        NetworkId::IPv4 => {}
        NetworkId::IPv6 => {}
        _ => {
            res.should_insta_ban = true;
            bail!("unsupported network type")
        }
    }

    let connection = connect_and_handshake(peer).await;
    if connection.is_err() {
        res.connect_handshake_error = Some(connection.unwrap_err());
        bail!("failed to connect/handshake")
    }
    let mut connection = connection.unwrap();
    addrman::update_peer_from_version(
        peer.clone(),
        connection.remote_version.services,
        connection.remote_version.start_height,
        connection.remote_version.user_agent.inner.to_vec(),
    )
    .await;
    connection
        .inner
        .write_packet(&PacketPayloadType::GetAddr(Cow::Owned(GetAddr {})))
        .await?;
    loop {
        {
            let packet = connection.inner.read_packet().await?;
            packet.payload.with_payload(|payload| {
                if let Some(payload) = payload {
                    handle_payload(payload);
                }
                res.packets_received_total += 1;
            });
        }
        addrman::update_peer_online(peer.clone(), connection.remote_version.services).await;
    }
}

fn handle_payload(payload: &PacketPayloadType) {
    match payload {
        PacketPayloadType::Addr(addys) => {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mut apns = Vec::with_capacity(addys.inner.len());
            for addy in addys.inner.iter() {
                let (network_id, addy_bytes) = match *addy.addr {
                    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, a, b, c, d] => {
                        (NetworkId::IPv4, vec![a, b, c, d])
                    }
                    other => (NetworkId::IPv6, other.to_vec()),
                };
                apns.push(AddressPortNetwork {
                    network_id,
                    port: addy.port,
                    address: addy_bytes,
                });
            }
            tokio::spawn(peers_seen(apns, time));
        }
        PacketPayloadType::AddrV2(addys) => {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mut apns = Vec::with_capacity(addys.inner.len());
            for addy in addys.inner.iter() {
                let (network_id, addy_bytes) = match (addy.network_id, addy.address.inner.len()) {
                    (NetworkId::IPv4, 4) => (NetworkId::IPv4, addy.address.inner.to_vec()),
                    (NetworkId::IPv6, 16) => match addy.address.inner.get(0..16).unwrap() {
                        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, a, b, c, d] => {
                            (NetworkId::IPv4, vec![*a, *b, *c, *d])
                        }
                        other => (NetworkId::IPv6, other.to_vec()),
                    },
                    _ => {
                        continue;
                    }
                };
                apns.push(AddressPortNetwork {
                    network_id,
                    port: addy.port,
                    address: addy_bytes,
                });
            }
            tokio::spawn(peers_seen(apns, time));
        }
        PacketPayloadType::Block(_block) => {
            // TODO
        }
        PacketPayloadType::Inv(_inv) => {
            // TODO
        }
        _ => {}
    }
}
