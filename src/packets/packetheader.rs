use anyhow::{Result, bail};
use bumpalo::{Bump, boxed::Box};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::tcp::ReadHalf,
};

use crate::packets::{
    magic::ACTIVE_MAGIC,
    packetpayload::Serializable,
    version::{VERSION_COMMAND, Version},
};

use super::{
    magic::Magic,
    packetpayload::{PacketPayload, PacketPayloadType},
};

pub struct PacketHeader {
    pub magic: Magic,
    pub command: [u8; 12],
    pub length: u32,
    pub checksum: u32,
}

pub async fn deserialize_packet<'bump, 'stream>(
    stream: &mut BufReader<ReadHalf<'stream>>,
    allocator: &'bump Bump,
) -> Result<(PacketHeader, PacketPayloadType<'bump>)> {
    let magic = stream.read_u32_le().await?;
    let mut command = [0u8; 12];
    stream.read_exact(&mut command).await?;
    let length = stream.read_u32_le().await?;
    let checksum = stream.read_u32().await?;

    let payload = match command {
        VERSION_COMMAND => {
            let mut v = Box::new_in(Version::default(), &allocator);
            v.deserialize(allocator, stream).await?;
            PacketPayloadType::Version(v)
        }
        other => {
            bail!("unknown command {}", hex::encode(command))
        }
    };

    Ok((
        PacketHeader {
            magic,
            command: command,
            length,
            checksum,
        },
        payload,
    ))
}

pub async fn write_packet_to_stream<'a, 'b>(
    packet: &'a impl PacketPayload<'a, 'b>,
    stream: &mut (impl AsyncWrite + Unpin),
) -> Result<()> {
    let mut buf = Vec::<u8>::with_capacity(32 * 1024 * 1024);
    packet.serialize(&mut buf);
    let mut hash = Sha256::digest(&buf);
    hash = Sha256::digest(hash);
    let shorthash = &hash.as_slice()[..4];

    stream.write_u32_le(ACTIVE_MAGIC).await?;
    stream.write(packet.command()).await?;
    stream.write_u32_le(buf.len() as u32).await?;
    stream.write(shorthash).await?;
    stream.write(&buf).await?;

    stream.flush().await?;

    Ok(())
}
