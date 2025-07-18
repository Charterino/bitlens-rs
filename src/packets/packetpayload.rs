use anyhow::{Result, bail};
use bumpalo::{Bump, collections::Vec};
use bytes::BufMut;
use sha2::{Digest, Sha256};
use std::fmt::{Debug, Display};
use supercow::Supercow;
use tokio::io::AsyncReadExt;

use super::{
    SupercowVec, addr::Addr, addrv2::AddrV2, block::Block, buffer::Buffer, deepclone::DeepClone,
    getaddr::GetAddr, getdata::GetData, getheaders::GetHeaders, headers::Headers, inv::Inv,
    packet::AllocatorWithBuffer, ping::Ping, pong::Pong, sendaddrv2::SendAddrV2,
    sendheaders::SendHeaders, tx::Tx, varint::VarInt, verack::VerAck, version::Version,
};

pub trait PacketPayload<'a, 'b: 'a>: Clone + Debug + DeepClone<'a, 'b> + Serializable<'a> {
    fn command(&self) -> &'static [u8; 12];
}

pub async fn read_payload(
    stream: &mut (impl AsyncReadExt + Unpin),
    buffer: &mut [u8],
) -> Result<()> {
    // Read the entire packet into buffer
    stream.read_exact(buffer).await?;
    Ok(())
}

#[derive(Debug)]
pub struct InvalidChecksum {
    pub reported_in_header: u32,
    pub calculated_from_body: u32,
}

impl Display for InvalidChecksum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "expected {:#X} got {:#X}",
            self.reported_in_header, self.calculated_from_body
        )
    }
}

pub fn deserialize_payload(
    allocator_with_buffer: &AllocatorWithBuffer,
    checksum: u32,
    command: [u8; 12],
) -> Result<Option<PacketPayloadType<'_>>> {
    let buffer = allocator_with_buffer.borrow_buffer();
    let allocator = allocator_with_buffer.borrow_allocator();
    let mut hash = Sha256::digest(buffer);
    hash = Sha256::digest(hash);
    let shorthash = hash.as_slice()[..4].try_into().unwrap();
    let shorthash = u32::from_le_bytes(shorthash);

    if shorthash != checksum {
        bail!(InvalidChecksum {
            reported_in_header: checksum,
            calculated_from_body: shorthash
        })
    }

    Ok(match command {
        super::version::VERSION_COMMAND => {
            let (v, _) = Version::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Version(v))
        }
        super::verack::VERACK_COMMAND => {
            let (v, _) = VerAck::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::VerAck(v))
        }
        super::ping::PING_COMMAND => {
            let (v, _) = Ping::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Ping(v))
        }
        super::pong::PONG_COMMAND => {
            let (v, _) = Pong::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Pong(v))
        }
        super::sendaddrv2::SENDADDRV2_COMMAND => {
            let (v, _) = SendAddrV2::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::SendAddrV2(v))
        }
        super::sendheaders::SENDHEADERS_COMMAND => {
            let (v, _) = SendHeaders::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::SendHeaders(v))
        }
        super::tx::TX_COMMAND => {
            let (v, _) = Tx::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Tx(v))
        }
        super::block::BLOCK_COMMAND => {
            let (v, _) = Block::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Block(v))
        }
        super::addr::ADDR_COMMAND => {
            let (v, _) = Addr::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Addr(v))
        }
        super::addrv2::ADDRV2_COMMAND => {
            let (v, _) = AddrV2::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::AddrV2(v))
        }
        super::inv::INV_COMMAND => {
            let (v, _) = Inv::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Inv(v))
        }
        super::headers::HEADERS_COMMAND => {
            let (v, _) = Headers::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::Headers(v))
        }
        super::getheaders::GETHEADERS_COMMAND => {
            let (v, _) = GetHeaders::deserialize(allocator, buffer)?;
            Some(PacketPayloadType::GetHeaders(v))
        }
        _ => None,
    })
}

pub trait Serializable<'bump>
where
    Self: Sized + Clone,
{
    // While the structs themselves are going to be allocated in the bump allocator,
    // the scripts and other byte-arrays are going to be referenced and not copied.
    fn deserialize(
        allocator: &'bump bumpalo::Bump<1>,
        buffer: &'bump [u8],
    ) -> Result<(Supercow<'bump, Self>, usize)>; // returned value is the length consumed

    fn serialize(&self, stream: &mut impl BufMut);
}

pub trait DeserializableOwned
where
    Self: Sized + Clone,
{
    fn deserialize_owned(buffer: &[u8]) -> Result<(Supercow<'static, Self>, usize)>; // returned value is the length consumed
}

pub trait SerializableValue<'bump>
where
    Self: Sized,
{
    // While the structs themselves are going to be allocated in the bump allocator,
    // the scripts and other byte-arrays are going to be referenced and not copied.
    fn deserialize(buffer: &'bump [u8]) -> Result<(Self, usize)>; // returned value is the length consumed

    fn serialize(&self, stream: &mut impl BufMut);
}

pub trait SerializableSupercowVecOfCows<'bump> {
    // While the structs themselves are going to be allocated in the bump allocator,
    // the scripts and other byte-arrays are going to be referenced and not copied.
    fn deserialize(
        allocator: &'bump bumpalo::Bump<1>,
        buffer: &'bump [u8],
    ) -> Result<(&'bump Self, usize)>
    where
        Self: Sized + Clone + ToOwned; // returned value is the length consumed

    fn serialize(&self, stream: &mut impl BufMut);
}

pub trait SerializableSupercowVecOfCowsOwned {
    // Everything is heap allocated
    fn deserialize_owned(buffer: &[u8]) -> Result<(Self, usize)>
    where
        Self: Sized + Clone + ToOwned; // returned value is the length consumed
}

#[derive(Clone, Debug)]
pub enum PacketPayloadType<'a> {
    Ping(Supercow<'a, Ping>),
    Inv(Supercow<'a, Inv<'a>>),
    Version(Supercow<'a, Version<'a>>),
    VerAck(Supercow<'a, VerAck>),
    Addr(Supercow<'a, Addr<'a>>),
    AddrV2(Supercow<'a, AddrV2<'a>>),
    GetAddr(Supercow<'a, GetAddr>),
    Pong(Supercow<'a, Pong>),
    SendHeaders(Supercow<'a, SendHeaders>),
    SendAddrV2(Supercow<'a, SendAddrV2>),
    Tx(Supercow<'a, Tx<'a>>),
    Block(Supercow<'a, Block<'a>>),
    GetData(Supercow<'a, GetData<'a>>),
    Headers(Supercow<'a, Headers<'a>>),
    GetHeaders(Supercow<'a, GetHeaders<'a>>),
}

impl PacketPayloadType<'_> {
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
            PacketPayloadType::Inv(inv) => inv.serialize(stream),
            PacketPayloadType::Headers(headers) => headers.serialize(stream),
            PacketPayloadType::GetHeaders(getheaders) => getheaders.serialize(stream),
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
            PacketPayloadType::Inv(inv) => inv.command(),
            PacketPayloadType::Headers(headers) => headers.command(),
            PacketPayloadType::GetHeaders(getheaders) => getheaders.command(),
        }
    }
}

impl<'a, T: Serializable<'a>> SerializableSupercowVecOfCows<'a> for SupercowVec<'a, T> {
    fn deserialize(allocator: &'a Bump<1>, buffer: &'a [u8]) -> Result<(&'a Self, usize)> {
        let (len, mut offset) = VarInt::deserialize(buffer)?;

        let mut result: Vec<'a, Supercow<'a, T>> = Vec::with_capacity_in(len as usize, allocator);
        for _ in 0..len as usize {
            let (value, offset_delta) = T::deserialize(allocator, buffer.with_offset(offset)?)?;
            offset += offset_delta;
            result.push(value);
        }

        let array = match allocator.try_alloc(SupercowVec {
            inner: Supercow::borrowed(result.into_bump_slice()),
        }) {
            Ok(v) => v,
            Err(e) => bail!(e),
        };
        Ok((array, offset))
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        let len = self.inner.len() as VarInt;
        len.serialize(stream);
        for i in 0..len as usize {
            self.inner[i].serialize(stream);
        }
    }
}

impl<T: DeserializableOwned> SerializableSupercowVecOfCowsOwned for SupercowVec<'static, T> {
    fn deserialize_owned(buffer: &[u8]) -> Result<(Self, usize)>
    where
        Self: Sized + Clone + ToOwned,
    {
        let (len, mut offset) = VarInt::deserialize(buffer)?;

        let mut result: std::vec::Vec<Supercow<'static, T>> =
            std::vec::Vec::with_capacity(len as usize);
        if result.try_reserve_exact(len as usize).is_err() {
            bail!("allocation failed");
        }
        for _ in 0..len as usize {
            let (value, offset_delta) = T::deserialize_owned(buffer.with_offset(offset)?)?;
            offset += offset_delta;
            result.push(value);
        }

        let array = SupercowVec {
            inner: Supercow::owned(result),
        };
        Ok((array, offset))
    }
}
