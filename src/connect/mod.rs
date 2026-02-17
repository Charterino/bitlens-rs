use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use connection::{Connection, HandshakedConnection};
use rand::RngCore;
use tokio::{
    io::{BufReader, BufWriter},
    net::tcp::OwnedReadHalf,
    select,
    sync::mpsc::{Sender, channel},
    time::{self, Instant},
};

use crate::{
    chainman,
    metrics::{METRIC_CONNECTIONS_IPV4, METRIC_CONNECTIONS_IPV6, METRIC_NET_DIALS_TOTAL},
    packets::{
        packet::{Packet, read_packet},
        packetpayload::{PayloadToSend, ReceivedPayload},
        pong::Pong,
        sendaddrv2::SendAddrV2,
        sendheaders::SendHeaders,
        verack::VerAck,
        version::VersionOwned,
    },
    types::addressportnetwork::AddressPortNetwork,
};

pub mod connection;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn connect_and_handshake(peer: &AddressPortNetwork) -> Result<HandshakedConnection> {
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

async fn handshake(conn: &mut Connection) -> Result<VersionOwned> {
    let version = VersionOwned {
        services: 9,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        nonce: rand::rng().next_u64(),
        user_agent: b"semikek".to_vec(),
        version: 70016,
        addrrecv: Default::default(),
        addrfrom: Default::default(),
        start_height: chainman::get_top_header_number() as u32,
        announce_relayed_transactions: false,
    };
    conn.write_packet(&PayloadToSend::Version(version)).await?;

    let mut remote_version: Option<VersionOwned> = None;
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
            for r in responses {
                conn.write_packet(&r).await?;
            }
        }
    }

    Ok(remote_version.unwrap())
}

fn handle_packet_during_handshaking(
    packet: &ReceivedPayload<'_>,
    success: &mut bool,
    remote_version: &mut Option<VersionOwned>,
) -> Result<Option<Vec<PayloadToSend>>> {
    match packet {
        ReceivedPayload::Version(v) => {
            if remote_version.is_some() {
                bail!("sent the version packet more than once")
            }
            let n = (**v).into();
            *remote_version = Some(n);
        }
        ReceivedPayload::VerAck(_) => {
            if remote_version.is_none() {
                bail!("sent verack before version")
            }
            let rv = remote_version.as_ref().unwrap();
            let mut res: Vec<PayloadToSend> = vec![];
            // BIP 155
            if rv.version > 70016 {
                res.push(PayloadToSend::SendAddrV2(SendAddrV2 {}));
            }
            res.push(PayloadToSend::VerAck(VerAck {}));
            // BIP 130
            if rv.version > 70012 {
                res.push(PayloadToSend::SendHeaders(SendHeaders {}));
            }
            *success = true;
            return Ok(Some(res));
        }
        ReceivedPayload::Ping(ping) => {
            return Ok(Some(vec![PayloadToSend::Pong(Pong { nonce: ping.nonce })]));
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

    let (tx, rx) = channel(8);
    let read_handle = tokio::spawn(read_and_send_messages(bufreader, tx));

    Ok(Connection {
        write_stream: bufwriter,
        read_handle,
        network_id: peer.network_id,
        read_chan: rx,
    })
}

async fn read_and_send_messages(
    mut read_stream: BufReader<OwnedReadHalf>,
    tx: Sender<Result<Packet>>,
) {
    loop {
        let packet = read_packet(&mut read_stream).await;
        let should_exit = packet.is_err();
        let _ = tx.send(packet).await;
        if should_exit {
            break;
        }
    }
}
