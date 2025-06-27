use std::ops::DerefMut;

use anyhow::Result;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncWriteExt, BufReader, BufWriter},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::{
    metrics::{METRIC_CONNECTIONS_IPV4, METRIC_CONNECTIONS_IPV6},
    packets::{
        magic::ACTIVE_MAGIC,
        network_id::NetworkId,
        packet::{Packet, SERIALIZE_POOL, read_packet},
        packetpayload::PacketPayloadType,
        version::Version,
    },
};

#[derive(Debug)]
pub struct Connection {
    pub read_stream: BufReader<OwnedReadHalf>,
    pub write_stream: BufWriter<OwnedWriteHalf>,
    pub network_id: NetworkId,
}

impl Drop for Connection {
    fn drop(&mut self) {
        match self.network_id {
            crate::packets::network_id::NetworkId::IPv4 => {
                METRIC_CONNECTIONS_IPV4.dec();
            }
            crate::packets::network_id::NetworkId::IPv6 => {
                METRIC_CONNECTIONS_IPV6.dec();
            }
            _ => {}
        }
    }
}

impl Connection {
    pub async fn write_packet(&mut self, packet: &PacketPayloadType<'_>) -> Result<()> {
        let mut serialize_buffer = SERIALIZE_POOL.get().await.unwrap();
        let buf = serialize_buffer.deref_mut();
        packet.serialize(buf);
        let mut hash = Sha256::digest(&buf);
        hash = Sha256::digest(hash);
        let shorthash = &hash.as_slice()[..4];

        self.write_stream.write_u32_le(ACTIVE_MAGIC).await?;
        self.write_stream.write_all(packet.command()).await?;
        self.write_stream.write_u32_le(buf.len() as u32).await?;
        self.write_stream.write_all(shorthash).await?;
        self.write_stream.write_all(buf).await?;

        self.write_stream.flush().await?;

        buf.clear();

        Ok(())
    }

    pub async fn read_packet(&mut self) -> Result<Packet> {
        read_packet(&mut self.read_stream).await
    }
}

#[derive(Debug)]
pub struct HandshakedConnection<'a> {
    pub inner: Connection,
    pub remote_version: Version<'a>,
}
