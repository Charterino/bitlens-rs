use std::ops::DerefMut;

use anyhow::Result;
use bumpalo::Bump;
use deadpool::unmanaged::Object;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncWriteExt, BufReader, BufWriter},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::packets::{
    magic::ACTIVE_MAGIC,
    packetheader::{PacketHeader, read_header},
    packetpayload::{Packet, PacketPayloadType, read_payload},
    version::Version,
};

use super::{DESERIALIZE_POOL, SERIALIZE_POOL};

#[derive(Debug)]
pub struct Connection {
    pub read_stream: BufReader<OwnedReadHalf>,
    pub write_stream: BufWriter<OwnedWriteHalf>,
}

impl Connection {
    pub async fn write_packet<'a>(&mut self, packet: &PacketPayloadType<'_>) -> Result<()> {
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

    pub async fn read_header(&mut self) -> Result<PacketHeader> {
        read_header(&mut self.read_stream).await
    }

    pub async fn prepare_for_read(&self) -> Object<Bump> {
        let mut v = DESERIALIZE_POOL.get().await.unwrap();
        v.reset();
        v
    }

    pub async fn read_packet<'a>(
        &mut self,
        header: PacketHeader,
        allocator: &'a mut Object<Bump>,
    ) -> Result<Packet<'a>> {
        let payload = read_payload(&mut self.read_stream, allocator, &header).await?;
        Ok(Packet { header, payload })
    }
}

#[derive(Debug)]
pub struct HandshakedConnection<'a> {
    pub inner: Connection,
    pub remote_version: Version<'a>,
}
