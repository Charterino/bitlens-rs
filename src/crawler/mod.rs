use crate::{
    addrman::{self, peers_seen},
    chainman::{
        self, SYNCING_BODIES, SYNCING_HEADERS, get_top_synced_header_hash,
        keepup::{BLOCK_APPLIED_BROADCAST, WORKER_REGISTRATION_CHANNEL},
    },
    connect::connect_and_handshake,
    crawler::peertracker::TrackedConnection,
    packets::{
        getaddr::GetAddr,
        getdata::GetDataOwned,
        getheaders::GetHeadersOwned,
        headers::HeadersOwned,
        inv::InvOwned,
        invvector::{InventoryVectorOwned, InventoryVectorType},
        network_id::NetworkId,
        packet::PayloadWithAllocator,
        packetpayload::{InvalidChecksum, PayloadToSend, ReceivedPayload},
        ping::Ping,
        pong::Pong,
    },
    types::addressportnetwork::AddressPortNetwork,
};
use anyhow::{Error, Result, bail};
use rand::RngCore;
use slog_scope::{debug, info};
use std::{
    sync::atomic::Ordering,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    select,
    sync::oneshot,
    time::{self, Instant, interval, sleep},
};

pub mod peertracker;

const NO_PEERS_SLEEP_DURATION: Duration = Duration::from_secs(1);
const SLEEP_BETWEEN_CRAWLS_DURATION: Duration = Duration::from_millis(20);
const PING_EVERY: Duration = Duration::from_secs(30);

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
    debug!("banning peer"; "peer" => peer.to_string(), "reasons" => fail_reasons.join(","));
    addrman::delete_and_ban_peer(peer, fail_reasons).await;
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
        match result
            .connect_handshake_error
            .as_ref()
            .unwrap()
            .downcast_ref::<InvalidChecksum>()
        {
            Some(_) => {
                // Instantly ban peers that do not follow the checksum protocol
                *failed_times += 14;
            }
            None => {
                *failed_times += 1;
            }
        }
        return Some(format!(
            "connect/handshake failed: {}",
            result.connect_handshake_error.as_ref().unwrap()
        ));
    }

    if disconnect_reason
        .downcast_ref::<InvalidChecksum>()
        .is_some()
    {
        // Instantly bad peers that do not follow the checksum requirements
        *failed_times = 14;
        return Some("invalid checksum".to_owned());
    }

    if result.should_insta_ban {
        *failed_times = 14;
        return Some(format!("insta_ban requested: {}", disconnect_reason));
    }

    if result.packets_received_total < 10 {
        // Some peers will accept connections, handshake, and disconnect.
        *failed_times += 1;
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
    let _track = TrackedConnection::new(peer.clone());
    addrman::on_peer_version_received(
        peer.clone(),
        connection.remote_version.services,
        connection.remote_version.start_height,
        connection.remote_version.user_agent,
    )
    .await;

    // Right after connecting send a GetAddr packet to learn about new peers
    // and send a GetHeaders packet so we can catch up to the main chain
    connection
        .inner
        .write_packet(&PayloadToSend::GetAddr(GetAddr {}))
        .await?;
    connection
        .inner
        .write_packet(&PayloadToSend::GetHeaders(GetHeadersOwned {
            version: 70016,
            block_locator: vec![get_top_synced_header_hash().unwrap_or_default()],
            hash_stop: [0u8; 32],
        }))
        .await?;
    let mut ping_interval = interval(PING_EVERY);
    let mut applied_block_chan = BLOCK_APPLIED_BROADCAST.0.subscribe();
    let (job_tx, mut job_rx) = oneshot::channel();
    WORKER_REGISTRATION_CHANNEL
        .get()
        .unwrap()
        .send(job_tx)
        .await
        .expect("to register peer as a worker");
    loop {
        select! {
            // Share hashes of blocks we verified and applied to our best chain
            hash = applied_block_chan.recv() => {
                let hash = hash?;
                connection.inner.write_packet(&PayloadToSend::Inv(InvOwned { inner: vec![InventoryVectorOwned { inv_type: InventoryVectorType::Block, hash }] })).await?;
            }
            // Send a ping packet once every PING_EVERY interval
            _ = ping_interval.tick() => {
                let nonce = rand::rng().next_u64();
                connection.inner.write_packet(&PayloadToSend::Ping(Ping {nonce})).await?;
            }
            // Handle the packets we receive from this peer
            packet = connection.inner.read_packet() => {
                let packet = packet?;
                {
                    let payload = packet.payload;
                    if let Some(responses) = handle_payload(payload).await {
                        for packet in responses {
                            connection.inner.write_packet(&packet).await?;
                        }
                    }
                    res.packets_received_total += 1;
                }
                addrman::update_peer_online(peer.clone(), connection.remote_version.services).await;
            }
            // Complete jobs assigned by the chainman
            job = &mut job_rx => {
                let job = job?;

                let should_process_headers = !SYNCING_HEADERS.load(Ordering::Relaxed);
                let should_process_blocks = should_process_headers && !SYNCING_BODIES.load(Ordering::Relaxed);

                // Only ask for blocks if we're not IBD'ing
                if should_process_blocks {
                    connection.inner.write_packet(&PayloadToSend::GetData(GetDataOwned { inner: vec![InventoryVectorOwned { inv_type: InventoryVectorType::WitnessBlock, hash: job }] })).await?;
                }

                // reregister
                let (new_job_tx, new_job_rx) = oneshot::channel();
                job_rx = new_job_rx;
                WORKER_REGISTRATION_CHANNEL
                    .get()
                    .unwrap()
                    .send(new_job_tx)
                    .await
                    .expect("to register peer as a worker");
            }
        }
    }
}

async fn handle_payload(payload: PayloadWithAllocator) -> Option<Vec<PayloadToSend>> {
    let should_process_headers = !SYNCING_HEADERS.load(Ordering::Relaxed);
    let should_process_blocks = should_process_headers && !SYNCING_BODIES.load(Ordering::Relaxed);
    if let Some(payload) = payload.borrow_payload() {
        match payload {
            ReceivedPayload::Ping(ping) => {
                return Some(vec![PayloadToSend::Pong(Pong { nonce: ping.nonce })]);
            }
            ReceivedPayload::Addr(addys) => {
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
                return None;
            }
            ReceivedPayload::AddrV2(addys) => {
                let time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let mut apns = Vec::with_capacity(addys.inner.len());
                for addy in addys.inner.iter() {
                    let (network_id, addy_bytes) = match (addy.network_id, addy.address.len()) {
                        (NetworkId::IPv4, 4) => (NetworkId::IPv4, addy.address.to_vec()),
                        (NetworkId::IPv6, 16) => match addy.address.get(0..16).unwrap() {
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
                return None;
            }
            ReceivedPayload::Block(block) => {
                if should_process_headers {
                    chainman::keepup::on_header_received(&block.header).await;
                }
                if !should_process_blocks {
                    return None;
                }
            }
            ReceivedPayload::Inv(inv) => {
                if !should_process_headers {
                    return None;
                }
                let mut blocks: Vec<InventoryVectorOwned> = vec![];
                for item in inv.inner {
                    if item.inv_type == InventoryVectorType::Block {
                        blocks.push(InventoryVectorOwned {
                            inv_type: InventoryVectorType::WitnessBlock,
                            hash: *item.hash,
                        });
                    }
                }
                return Some(vec![PayloadToSend::GetData(GetDataOwned { inner: blocks })]);
            }
            ReceivedPayload::Headers(headers) => {
                if !should_process_headers {
                    return None;
                }
                for header in headers.inner {
                    chainman::keepup::on_header_received(header).await;
                }
                return None;
            }
            ReceivedPayload::GetHeaders(get_headers) => {
                if should_process_headers {
                    return chainman::respond_to_getheaders(
                        get_headers.block_locator,
                        *get_headers.hash_stop,
                    )
                    .map(|v| vec![PayloadToSend::Headers(HeadersOwned { inner: v })]);
                } else {
                    return None;
                }
            }
            _ => {
                return None;
            }
        }
    } else {
        return None;
    };
    // if we got here, that means its a Block and we must send it to chainman
    chainman::keepup::on_block_received(payload).await;
    None
}
