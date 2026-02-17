use std::ops::DerefMut;

use anyhow::{Result, anyhow};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncWriteExt, BufWriter},
    net::tcp::OwnedWriteHalf,
    sync::mpsc::Receiver,
    task::JoinHandle,
};

use crate::{
    metrics::{METRIC_CONNECTIONS_IPV4, METRIC_CONNECTIONS_IPV6},
    packets::{
        magic::ACTIVE_MAGIC,
        network_id::NetworkId,
        packet::{Packet, SERIALIZE_POOL},
        packetpayload::PayloadToSend,
        version::VersionOwned,
    },
};

#[derive(Debug)]
pub struct Connection {
    pub read_handle: JoinHandle<()>,
    pub write_stream: BufWriter<OwnedWriteHalf>,
    pub network_id: NetworkId,
    pub read_chan: Receiver<Result<Packet>>,
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
    pub async fn write_packet(&mut self, packet: &PayloadToSend) -> Result<()> {
        let mut serialize_buffer = SERIALIZE_POOL.get().await?;
        let buf = serialize_buffer.deref_mut();
        buf.clear();
        packet.serialize(buf);
        let mut hash = Sha256::digest(&buf);
        hash = Sha256::digest(hash);
        let shorthash = &hash[..4];

        self.write_stream.write_u32_le(ACTIVE_MAGIC).await?;
        self.write_stream.write_all(packet.command()).await?;
        self.write_stream.write_u32_le(buf.len() as u32).await?;
        self.write_stream.write_all(shorthash).await?;
        self.write_stream.write_all(buf).await?;

        self.write_stream.flush().await?;

        Ok(())
    }

    pub async fn read_packet(&mut self) -> Result<Packet> {
        
        match self.read_chan.recv().await {
            Some(v) => v,
            None => Err(anyhow!("read chan closed")),
        }
    }
}

#[derive(Debug)]
pub struct HandshakedConnection {
    pub inner: Connection,
    pub remote_version: VersionOwned,
}
