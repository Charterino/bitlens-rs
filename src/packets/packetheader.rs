use anyhow::Result;
use bumpalo::Bump;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::tcp::ReadHalf,
};

use crate::packets::{
    block::{BLOCK_COMMAND, Block},
    magic::ACTIVE_MAGIC,
    packetpayload::Serializable,
    ping::{PING_COMMAND, Ping},
    pong::{PONG_COMMAND, Pong},
    sendaddrv2::{SENDADDRV2_COMMAND, SendAddrV2},
    sendheaders::{SENDHEADERS_COMMAND, SendHeaders},
    tx::{TX_COMMAND, Tx},
    version::{VERSION_COMMAND, Version},
};

use super::{
    magic::Magic,
    packetpayload::{PacketPayload, PacketPayloadType},
    verack::{VERACK_COMMAND, VerAck},
};

pub struct PacketHeader {
    pub magic: Magic,
    pub command: [u8; 12],
    pub length: u32,
    pub checksum: u32,
}

pub async fn deserialize_packet<'bump>(
    stream: &mut BufReader<ReadHalf<'_>>,
    allocator: &'bump Bump,
) -> Result<(PacketHeader, Option<PacketPayloadType<'bump>>)> {
    let magic = AsyncReadExt::read_u32_le(stream).await?;
    let mut command = [0u8; 12];
    AsyncReadExt::read_exact(stream, &mut command).await?;
    let length = AsyncReadExt::read_u32_le(stream).await?;
    let mut checksum = [0u8; 4];
    AsyncReadExt::read_exact(stream, &mut checksum).await?;
    println!(
        "magic: {} payload length: {} command: {}",
        hex::encode(magic.to_le_bytes()),
        length,
        String::from_utf8_lossy(&command)
    );

    let buffer = allocator.alloc_slice_fill_default::<u8>(length as usize);

    // Read the entire packet into buffer
    stream.read_exact(buffer).await?;

    let mut hash = Sha256::digest(&buffer);
    hash = Sha256::digest(hash);
    let shorthash = &hash.as_slice()[..4];

    println!(
        "expected {} got {}",
        hex::encode(checksum),
        hex::encode(shorthash)
    );

    let payload = match command {
        VERSION_COMMAND => {
            let (v, _) = Version::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Version(v))
        }
        VERACK_COMMAND => {
            let (v, _) = VerAck::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::VerAck(v))
        }
        PING_COMMAND => {
            let (v, _) = Ping::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Ping(v))
        }
        PONG_COMMAND => {
            let (v, _) = Pong::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Pong(v))
        }
        SENDADDRV2_COMMAND => {
            let (v, _) = SendAddrV2::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::SendAddrV2(v))
        }
        SENDHEADERS_COMMAND => {
            let (v, _) = SendHeaders::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::SendHeaders(v))
        }
        TX_COMMAND => {
            let (v, _) = Tx::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Tx(v))
        }
        BLOCK_COMMAND => {
            let (v, _) = Block::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Block(v))
        }
        _ => {
            println!(
                "unknown command {} {}",
                String::from_utf8_lossy(&command),
                hex::encode(command)
            );
            None
        }
    };

    Ok((
        PacketHeader {
            magic,
            command,
            length,
            checksum: u32::from_le_bytes(checksum),
        },
        payload,
    ))
}

pub async fn write_packet_to_stream<'a>(
    packet: &'a impl PacketPayload<'a>,
    stream: &mut (impl AsyncWrite + Unpin),
) -> Result<()> {
    let mut buf = Vec::<u8>::with_capacity(32 * 1024 * 1024);
    packet.serialize(&mut buf);
    let mut hash = Sha256::digest(&buf);
    hash = Sha256::digest(hash);
    let shorthash = &hash.as_slice()[..4];

    stream.write_u32_le(ACTIVE_MAGIC).await?;
    stream.write_all(packet.command()).await?;
    stream.write_u32_le(buf.len() as u32).await?;
    stream.write_all(shorthash).await?;
    stream.write_all(&buf).await?;

    stream.flush().await?;

    Ok(())
}
