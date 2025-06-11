use std::{borrow::Cow, fmt::Debug};

use anyhow::{Result, bail};
use bumpalo::{Bump, collections::Vec};
use bytes::BufMut;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use super::{
    addr::Addr, addrv2::AddrV2, block::Block, buffer::Buffer, deepclone::DeepClone,
    getaddr::GetAddr, getdata::GetData, packetheader::PacketHeader, ping::Ping, pong::Pong,
    sendaddrv2::SendAddrV2, sendheaders::SendHeaders, tx::Tx, varint::VarInt, verack::VerAck,
    version::Version,
};

pub struct Packet<'a> {
    pub header: PacketHeader,
    pub payload: Option<PacketPayloadType<'a>>,
}

pub trait PacketPayload<'a, 'b: 'a>:
    Clone + Debug + Default + DeepClone<'a, 'b> + Serializable<'a>
{
    fn command(&self) -> &'static [u8; 12];
}

pub async fn read_payload<'bump>(
    stream: &mut (impl AsyncReadExt + Unpin),
    allocator: &'bump mut Bump,
    header: &PacketHeader,
) -> Result<Option<PacketPayloadType<'bump>>> {
    let buffer = allocator.alloc_slice_fill_default::<u8>(header.length as usize);

    // Read the entire packet into buffer
    stream.read_exact(buffer).await?;

    let mut hash = Sha256::digest(&buffer);
    hash = Sha256::digest(hash);
    let shorthash = hash.as_slice()[..4].try_into().unwrap();
    let shorthash = u32::from_le_bytes(shorthash);

    if shorthash != header.checksum {
        bail!(
            "invalid checksum, expected {} got {}",
            header.checksum,
            shorthash
        )
    }

    Ok(match header.command {
        super::version::VERSION_COMMAND => {
            let (v, _) = Version::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Version(Cow::Borrowed(v)))
        }
        super::verack::VERACK_COMMAND => {
            let (v, _) = VerAck::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::VerAck(Cow::Borrowed(v)))
        }
        super::ping::PING_COMMAND => {
            let (v, _) = Ping::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Ping(Cow::Borrowed(v)))
        }
        super::pong::PONG_COMMAND => {
            let (v, _) = Pong::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Pong(Cow::Borrowed(v)))
        }
        super::sendaddrv2::SENDADDRV2_COMMAND => {
            let (v, _) = SendAddrV2::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::SendAddrV2(Cow::Borrowed(v)))
        }
        super::sendheaders::SENDHEADERS_COMMAND => {
            let (v, _) = SendHeaders::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::SendHeaders(Cow::Borrowed(v)))
        }
        super::tx::TX_COMMAND => {
            let (v, _) = Tx::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Tx(Cow::Borrowed(v)))
        }
        super::block::BLOCK_COMMAND => {
            let (v, _) = Block::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Block(Cow::Borrowed(v)))
        }
        super::addr::ADDR_COMMAND => {
            let (v, _) = Addr::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Addr(Cow::Borrowed(v)))
        }
        super::addrv2::ADDRV2_COMMAND => {
            let (v, _) = AddrV2::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::AddrV2(Cow::Borrowed(v)))
        }
        _ => None,
    })
}

pub trait Serializable<'bump>
where
    Self: Sized,
{
    // While the structs themselves are going to be allocated in the bump allocator,
    // the scripts and other byte-arrays are going to be referenced and not copied.
    fn deserialize(
        allocator: &'bump bumpalo::Bump<1>,
        buffer: &'bump [u8],
    ) -> Result<(&'bump Self, usize)>; // returned value is the length consumed

    fn serialize(&self, stream: &mut impl BufMut);
}

pub trait SerializableValue<'bump>
where
    Self: Sized,
{
    // While the structs themselves are going to be allocated in the bump allocator,
    // the scripts and other byte-arrays are going to be referenced and not copied.
    fn deserialize(
        allocator: &'bump bumpalo::Bump<1>,
        buffer: &'bump [u8],
    ) -> Result<(Self, usize)>; // returned value is the length consumed

    fn serialize(&self, stream: &mut impl BufMut);
}

#[derive(Clone, Debug)]
pub enum PacketPayloadType<'a> {
    Version(Cow<'a, Version<'a>>),
    VerAck(Cow<'a, VerAck>),
    Addr(Cow<'a, Addr<'a>>),
    AddrV2(Cow<'a, AddrV2<'a>>),
    GetAddr(Cow<'a, GetAddr>),
    Ping(Cow<'a, Ping>),
    Pong(Cow<'a, Pong>),
    SendHeaders(Cow<'a, SendHeaders>),
    SendAddrV2(Cow<'a, SendAddrV2>),
    Tx(Cow<'a, Tx<'a>>),
    Block(Cow<'a, Block<'a>>),
    GetData(Cow<'a, GetData<'a>>),
}

impl<'a> PacketPayloadType<'a> {
    pub fn serialize(&self, stream: &mut std::vec::Vec<u8>) {
        match self {
            PacketPayloadType::Version(version) => version.serialize(stream),
            PacketPayloadType::VerAck(ver_ack) => ver_ack.serialize(stream),
            PacketPayloadType::Ping(ping) => ping.serialize(stream),
            PacketPayloadType::Pong(pong) => pong.serialize(stream),
            PacketPayloadType::SendHeaders(send_headers) => send_headers.serialize(stream),
            PacketPayloadType::SendAddrV2(send_addr_v2) => send_addr_v2.serialize(stream),
            PacketPayloadType::Tx(tx) => tx.serialize(stream),
            PacketPayloadType::Block(block) => block.serialize(stream),
            PacketPayloadType::Addr(addr) => addr.serialize(stream),
            PacketPayloadType::AddrV2(addrv2) => addrv2.serialize(stream),
            PacketPayloadType::GetAddr(getaddr) => getaddr.serialize(stream),
            PacketPayloadType::GetData(getdata) => getdata.serialize(stream),
        }
    }

    pub fn command(&self) -> &'static [u8; 12] {
        match self {
            PacketPayloadType::Version(version) => version.command(),
            PacketPayloadType::VerAck(ver_ack) => ver_ack.command(),
            PacketPayloadType::Ping(ping) => ping.command(),
            PacketPayloadType::Pong(pong) => pong.command(),
            PacketPayloadType::SendHeaders(send_headers) => send_headers.command(),
            PacketPayloadType::SendAddrV2(send_addr_v2) => send_addr_v2.command(),
            PacketPayloadType::Tx(tx) => tx.command(),
            PacketPayloadType::Block(block) => block.command(),
            PacketPayloadType::Addr(addr) => addr.command(),
            PacketPayloadType::AddrV2(addrv2) => addrv2.command(),
            PacketPayloadType::GetAddr(getaddr) => getaddr.command(),
            PacketPayloadType::GetData(getdata) => getdata.command(),
        }
    }
}

impl<'a, T: Serializable<'a> + ToOwned> SerializableValue<'a> for Cow<'a, [Cow<'a, T>]> {
    fn deserialize(allocator: &'a Bump<1>, buffer: &'a [u8]) -> Result<(Self, usize)> {
        let (len, mut offset) = VarInt::deserialize(allocator, buffer)?;

        let mut result: Vec<'a, Cow<'a, T>> = Vec::with_capacity_in(len as usize, allocator);
        for _ in 0..len as usize {
            let (value, offset_delta) = T::deserialize(allocator, buffer.with_offset(offset)?)?;
            offset += offset_delta;
            result.push(Cow::Borrowed(value));
        }

        Ok((Cow::Borrowed(result.into_bump_slice()), offset))
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        let len = self.len() as VarInt;
        len.serialize(stream);
        for i in 0..len as usize {
            self.get(i).unwrap().serialize(stream);
        }
    }
}
