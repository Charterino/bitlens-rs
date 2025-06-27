use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use connection::{Connection, HandshakedConnection};
use rand::RngCore;
use supercow::Supercow;
use tokio::{
    io::{BufReader, BufWriter},
    select,
    time::{self, Instant},
};

use crate::{
    metrics::{METRIC_CONNECTIONS_IPV4, METRIC_CONNECTIONS_IPV6, METRIC_NET_DIALS_TOTAL},
    packets::{
        deepclone::DeepClone, packetpayload::PacketPayloadType, sendaddrv2::SendAddrV2,
        sendheaders::SendHeaders, varstr::VarStr, verack::VerAck, version::Version,
    },
    types::addressportnetwork::AddressPortNetwork,
};

pub mod connection;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn connect_and_handshake(
    peer: &AddressPortNetwork,
) -> Result<HandshakedConnection<'static>> {
    let mut conn = connect(peer).await?;

    let deadline = Instant::now().checked_add(HANDSHAKE_TIMEOUT).unwrap();

    select! {
        _ = time::sleep_until(deadline) => {
            // Deadline reached and we still havent handshaked! What is this peer...
            bail!("handshake deadline reached")
        },
        handshake_result = handshake(&mut conn) => {
            match handshake_result {
                Err(e) => Err(e),
                Ok(remote_version) => Ok(HandshakedConnection { inner: conn, remote_version })
            }
        }
    }
}

async fn handshake(conn: &mut Connection) -> Result<Version<'static>> {
    let version = Version {
        services: 9,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        nonce: rand::rng().next_u64(),
        user_agent: Supercow::owned(VarStr::from("semikek")),
        version: 70016,
        ..Default::default()
    };
    conn.write_packet(&PacketPayloadType::Version(Supercow::owned(version)))
        .await?;

    let mut remote_version: Option<Version> = None;
    let mut finished = false;

    while !finished {
        let packet = conn.read_packet().await?;
        let responses = packet.payload.with_payload(|payload| {
            if let Some(p) = payload {
                handle_packet_during_handshaking(p, &mut finished, &mut remote_version)
            } else {
                Ok(None)
            }
        })?;
        drop(packet);
        if let Some(responses) = responses {
            for r in &responses {
                conn.write_packet(r).await?;
            }
        }
    }

    Ok(remote_version.unwrap())
}

fn handle_packet_during_handshaking<'packet, 'ret: 'packet, 'params: 'ret>(
    packet: &PacketPayloadType<'packet>,
    success: &mut bool,
    remote_version: &mut Option<Version<'params>>,
) -> Result<Option<Vec<PacketPayloadType<'ret>>>> {
    match packet {
        PacketPayloadType::Version(v) => {
            if remote_version.is_some() {
                bail!("sent the version packet more than once")
            }
            let n = v.deep_clone();
            *remote_version = Some(n);
        }
        PacketPayloadType::VerAck(_) => {
            if remote_version.is_none() {
                bail!("sent verack before version")
            }
            let rv = remote_version.as_ref().unwrap();
            let mut res: Vec<PacketPayloadType<'ret>> = vec![];
            // BIP 155
            if rv.version > 70016 {
                res.push(PacketPayloadType::SendAddrV2(Supercow::owned(
                    SendAddrV2 {},
                )));
            }
            res.push(PacketPayloadType::VerAck(Supercow::owned(VerAck {})));
            // BIP 130
            if rv.version > 70012 {
                res.push(PacketPayloadType::SendHeaders(Supercow::owned(
                    SendHeaders {},
                )));
            }
            *success = true;
            return Ok(Some(res));
        }
        _ => {}
    }
    Ok(None)
}

async fn connect(peer: &AddressPortNetwork) -> Result<Connection> {
    METRIC_NET_DIALS_TOTAL.inc();
    let stream = tokio::net::TcpStream::connect(peer.to_string()).await?;
    let (a, b) = stream.into_split();
    let bufwriter = BufWriter::new(b);
    let bufreader = BufReader::new(a);

    match peer.network_id {
        crate::packets::network_id::NetworkId::IPv4 => {
            METRIC_CONNECTIONS_IPV4.inc();
        }
        crate::packets::network_id::NetworkId::IPv6 => {
            METRIC_CONNECTIONS_IPV6.inc();
        }
        _ => {}
    }

    Ok(Connection {
        write_stream: bufwriter,
        read_stream: bufreader,
        network_id: peer.network_id,
    })
}
