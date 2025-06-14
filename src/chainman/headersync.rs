use std::{borrow::Cow, sync::atomic::Ordering, time::Duration};

use crate::{
    addrman,
    chainman::{CHAIN, validate_and_apply_header},
    packets::{getheaders::GetHeaders, packetpayload::PacketPayloadType},
    with_deadline,
};
use anyhow::Result;
use slog_scope::info;
use tokio::{select, time::Instant};

use super::{SYNCING_HEADERS, build_get_headers, need_ihd};

const HEADERS_TIMEOUT: Duration = Duration::from_secs(3);

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
            let header = match with_deadline!(connection.inner.read_header(), deadline) {
                Ok(h) => h,
                Err(_) => {
                    need_break = true;
                    continue;
                }
            };
            let mut allocator = connection.inner.prepare_for_read().await;
            let packet = match with_deadline!(
                connection.inner.read_packet(header, &mut allocator),
                deadline
            ) {
                Ok(p) => p,
                Err(_) => {
                    need_break = true;
                    continue;
                }
            };
            if let Some(payload) = packet.payload {
                let responses = match handle_packet_during_headersync(
                    payload,
                    &mut need_break,
                    &mut deadline,
                ) {
                    Ok(responses) => responses,
                    Err(_) => {
                        need_break = true;
                        continue;
                    }
                };
                drop(allocator);
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
    }
    SYNCING_HEADERS.store(false, Ordering::Relaxed);
}

fn handle_packet_during_headersync<'a, 'c, 'b: 'c>(
    packet: PacketPayloadType<'c>,
    need_break: &mut bool,
    deadline: &mut Instant,
) -> Result<Option<Vec<PacketPayloadType<'a>>>> {
    match packet {
        PacketPayloadType::Headers(headers) => {
            if headers.inner.len() == 0 {
                *need_break = true;
                return Ok(None);
            }
            for header in headers.inner.iter() {
                if validate_and_apply_header(header).is_err() {
                    *need_break = true;
                    return Ok(None);
                }
            }
            let r = CHAIN.read().unwrap();
            info!("chainman: header sync progress"; "top hash" => r.top_header.header.human_hash(), "top number" => r.top_header.number);
            *deadline = Instant::now().checked_add(HEADERS_TIMEOUT).unwrap();
            if need_ihd() {
                return Ok(Some(vec![PacketPayloadType::GetHeaders(Cow::Owned(
                    GetHeaders {
                        version: 70016,
                        block_locator: Cow::Owned(vec![Cow::Owned(
                            headers.inner.last().unwrap().hash,
                        )]),
                        hash_stop: Cow::Owned([0u8; 32]),
                    },
                ))]));
            } else {
                *need_break = true;
                return Ok(None);
            }
        }
        _ => {}
    }
    Ok(None)
}
