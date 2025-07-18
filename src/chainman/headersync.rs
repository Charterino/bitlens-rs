use std::{sync::atomic::Ordering, time::Duration};

use crate::{
    addrman,
    chainman::{CHAIN, validate_and_apply_headers},
    ok_or_break,
    packets::{
        SupercowVec, blockheader::BlockHeader, getheaders::GetHeaders,
        packetpayload::PacketPayloadType,
    },
    with_deadline,
};
use slog_scope::info;
use supercow::Supercow;
use tokio::time::Instant;

use super::{SYNCING_HEADERS, build_get_headers, need_ihd};

const HEADERS_TIMEOUT: Duration = Duration::from_millis(500);

pub async fn sync_headers() {
    SYNCING_HEADERS.store(true, Ordering::Relaxed);
    while need_ihd() {
        let mut connection = addrman::connect_to_good_peer(None).await;
        let get_headers = build_get_headers();
        if connection.inner.write_packet(&get_headers).await.is_err() {
            continue;
        }
        let mut deadline = Instant::now().checked_add(HEADERS_TIMEOUT).unwrap();
        let mut need_break = false;
        while !need_break {
            let packet = ok_or_break!(with_deadline!(connection.inner.read_packet(), deadline));
            let responses = packet.payload.with_payload(|payload| {
                if let Some(p) = payload {
                    handle_packet_during_headersync(p, &mut need_break, &mut deadline)
                } else {
                    None
                }
            });
            drop(packet);

            if let Some(responses) = responses {
                for r in &responses {
                    if connection.inner.write_packet(r).await.is_err() {
                        need_break = true;
                        continue;
                    }
                }
            }
        }
    }
    SYNCING_HEADERS.store(false, Ordering::Relaxed);
}

fn handle_packet_during_headersync<'packet, 'ret: 'packet, 'params: 'ret>(
    packet: &PacketPayloadType<'packet>,
    need_break: &mut bool,
    deadline: &mut Instant,
) -> Option<Vec<PacketPayloadType<'ret>>> {
    if let PacketPayloadType::Headers(headers) = packet {
        if headers.inner.inner.is_empty() {
            *need_break = true;
            return None;
        }
        if validate_and_apply_headers(
            &headers
                .inner
                .inner
                .iter()
                .map(|x| x.as_ref())
                .collect::<Vec<&BlockHeader>>(),
        )
        .is_err()
        {
            *need_break = true;
            return None;
        }
        let r = CHAIN.read().unwrap();
        info!("chainman: header sync progress"; "top hash" => r.top_header.header.human_hash(), "top number" => r.top_header.number);
        *deadline = Instant::now().checked_add(HEADERS_TIMEOUT).unwrap();
        if need_ihd() {
            return Some(vec![PacketPayloadType::GetHeaders(Supercow::owned(
                GetHeaders {
                    version: 70016,
                    block_locator: Supercow::owned(SupercowVec {
                        inner: Supercow::owned(vec![Supercow::owned(
                            headers.inner.inner.last().unwrap().hash,
                        )]),
                    }),
                    hash_stop: Supercow::owned([0u8; 32]),
                },
            ))]);
        } else {
            *need_break = true;
            return None;
        }
    }
    None
}
