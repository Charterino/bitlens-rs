use std::{
    borrow::Cow,
    sync::LazyLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, bail};
use bumpalo::Bump;
use connection::{Connection, HandshakedConnection};
use deadpool::unmanaged::Pool;
use rand::RngCore;
use tokio::{
    io::{BufReader, BufWriter},
    select,
    time::{self, Instant},
};

use crate::{
    packets::{
        MAX_PACKET_SIZE, deepclone::DeepClone, packetpayload::PacketPayloadType,
        sendaddrv2::SendAddrV2, sendheaders::SendHeaders, varstr::VarStr, verack::VerAck,
        version::Version,
    },
    types::addressportnetwork::AddressPortNetwork,
};

pub mod connection;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

static SERIALIZE_POOL: LazyLock<Pool<Vec<u8>>> = LazyLock::new(|| {
    let mut pool = Vec::with_capacity(32);
    for _ in 0..32 {
        pool.push(Vec::with_capacity(MAX_PACKET_SIZE));
    }
    Pool::from(pool)
});

static DESERIALIZE_POOL: LazyLock<Pool<Bump<1>>> = LazyLock::new(|| {
    let mut pool = Vec::with_capacity(1024);
    for _ in 0..1024 {
        let b = Bump::with_capacity(MAX_PACKET_SIZE);
        b.set_allocation_limit(Some(MAX_PACKET_SIZE));
        pool.push(b);
    }
    Pool::from(pool)
});

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

async fn handshake<'a>(conn: &mut Connection) -> Result<Version<'a>> {
    let version = Version {
        services: 9,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        nonce: rand::rng().next_u64(),
        user_agent: VarStr::from("semikek"),
        version: 70016,
        ..Default::default()
    };
    conn.write_packet(&PacketPayloadType::Version(Cow::Owned(version)))
        .await?;

    let mut remote_version: Option<Version<'a>> = None;
    let mut finished = false;

    while !finished {
        let header = conn.read_header().await?;
        let mut allocator = conn.prepare_for_read().await;
        let packet = conn.read_packet(header, &mut allocator).await?;
        if let Some(payload) = packet.payload {
            let responses =
                handle_packet_during_handshaking(payload, &mut finished, &mut remote_version)?;
            drop(allocator);
            if let Some(responses) = responses {
                for r in &responses {
                    conn.write_packet(r).await?;
                }
            }
        }
    }

    Ok(remote_version.unwrap())
}

fn handle_packet_during_handshaking<'a, 'c, 'b: 'c>(
    packet: PacketPayloadType<'c>,
    success: &mut bool,
    remote_version: &mut Option<Version<'b>>,
) -> Result<Option<Vec<PacketPayloadType<'a>>>> {
    match packet {
        PacketPayloadType::Version(v) => {
            if remote_version.is_some() {
                bail!("sent the version packet more than once")
            }
            let n: Version<'b> = v.deep_clone();
            *remote_version = Some(n)
        }
        PacketPayloadType::VerAck(_) => {
            if remote_version.is_none() {
                bail!("sent verack before version")
            }
            let rv = remote_version.as_ref().unwrap();
            let mut res = vec![];
            // BIP 155
            if rv.version > 70016 {
                res.push(PacketPayloadType::SendAddrV2(Cow::Owned(SendAddrV2 {})));
            }
            res.push(PacketPayloadType::VerAck(Cow::Owned(VerAck {})));
            // BIP 130
            if rv.version > 70012 {
                res.push(PacketPayloadType::SendHeaders(Cow::Owned(SendHeaders {})));
            }
            *success = true;
            return Ok(Some(res));
        }
        _ => {}
    }
    Ok(None)
}

async fn connect(peer: &AddressPortNetwork) -> Result<Connection> {
    let stream = tokio::net::TcpStream::connect(peer.to_string()).await?;
    let (a, b) = stream.into_split();
    let bufwriter = BufWriter::new(b);
    let bufreader = BufReader::new(a);

    Ok(Connection {
        write_stream: bufwriter,
        read_stream: bufreader,
    })
}
