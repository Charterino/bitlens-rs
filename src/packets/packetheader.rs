use anyhow::Result;
use bumpalo::{Bump, boxed::Box};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::tcp::ReadHalf,
};

use crate::packets::{
    addr::{ADDR_COMMAND, Addr},
    addrv2::{ADDRV2_COMMAND, AddrV2},
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

pub async fn deserialize_packet<'bump, 'stream>(
    stream: &mut BufReader<ReadHalf<'stream>>,
    allocator: &'bump Bump,
) -> Result<(PacketHeader, Option<PacketPayloadType<'bump>>)> {
    let magic = stream.read_u32_le().await?;
    let mut command = [0u8; 12];
    stream.read_exact(&mut command).await?;
    let length = stream.read_u32_le().await?;
    let checksum = stream.read_u32().await?;
    println!(
        "magic: {} payload length: {} command: {}",
        hex::encode(magic.to_le_bytes()),
        length,
        String::from_utf8_lossy(&command)
    );

    let payload = match command {
        VERSION_COMMAND => {
            let mut v: Box<'bump, Version<'bump>> = Box::new_in(Version::default(), allocator);
            v.deserialize(allocator, stream).await?;
            Some(PacketPayloadType::Version(v))
        }
        VERACK_COMMAND => {
            let mut v = Box::new_in(VerAck::default(), allocator);
            v.deserialize(allocator, stream).await?;
            Some(PacketPayloadType::VerAck(v))
        }
        PING_COMMAND => {
            let mut v = Box::new_in(Ping::default(), allocator);
            v.deserialize(allocator, stream).await?;
            Some(PacketPayloadType::Ping(v))
        }
        PONG_COMMAND => {
            let mut v = Box::new_in(Pong::default(), allocator);
            v.deserialize(allocator, stream).await?;
            Some(PacketPayloadType::Pong(v))
        }
        ADDR_COMMAND => {
            let mut v = Box::new_in(Addr::default(), allocator);
            v.deserialize(allocator, stream).await?;
            Some(PacketPayloadType::Addr(v))
        }
        ADDRV2_COMMAND => {
            let mut v = Box::new_in(AddrV2::default(), allocator);
            v.deserialize(allocator, stream).await?;
            Some(PacketPayloadType::AddrV2(v))
        }
        SENDADDRV2_COMMAND => {
            let mut v = Box::new_in(SendAddrV2::default(), allocator);
            v.deserialize(allocator, stream).await?;
            Some(PacketPayloadType::SendAddrV2(v))
        }
        SENDHEADERS_COMMAND => {
            let mut v = Box::new_in(SendHeaders::default(), allocator);
            v.deserialize(allocator, stream).await?;
            Some(PacketPayloadType::SendHeaders(v))
        }
        TX_COMMAND => {
            let mut v = Box::new_in(Tx::default(), allocator);
            v.deserialize(allocator, stream).await?;
            Some(PacketPayloadType::Tx(v))
        }
        BLOCK_COMMAND => {
            let mut v = Box::new_in(Block::default(), allocator);
            v.deserialize(allocator, stream).await?;
            Some(PacketPayloadType::Block(v))
        }
        other => {
            println!(
                "unknown command {} {}",
                String::from_utf8_lossy(&command),
                hex::encode(command)
            );
            tokio::io::copy(&mut stream.take(length as u64), &mut tokio::io::sink()).await?;
            None
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
