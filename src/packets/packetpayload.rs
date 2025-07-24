use anyhow::{Result, bail};
use bytes::BufMut;
use sha2::{Digest, Sha256};
use std::fmt::{Debug, Display};
use tokio::io::AsyncReadExt;

use crate::util::arena::Arena;

use super::{
    addr::{ADDR_COMMAND, AddrBorrowed, AddrOwned},
    addrv2::{ADDRV2_COMMAND, AddrV2Borrowed, AddrV2Owned},
    block::{BLOCK_COMMAND, BlockBorrowed, BlockOwned},
    getaddr::{GETADDR_COMMAND, GetAddr},
    getdata::{GETDATA_COMMAND, GetDataBorrowed, GetDataOwned},
    getheaders::{GETHEADERS_COMMAND, GetHeadersBorrowed, GetHeadersOwned},
    headers::{HEADERS_COMMAND, HeadersBorrowed, HeadersOwned},
    inv::{INV_COMMAND, InvBorrowed, InvOwned},
    packet::AllocatorWithBuffer,
    ping::{PING_COMMAND, Ping},
    pong::{PONG_COMMAND, Pong},
    sendaddrv2::{SENDADDRV2_COMMAND, SendAddrV2},
    sendheaders::{SENDHEADERS_COMMAND, SendHeaders},
    tx::{TX_COMMAND, TxBorrowed, TxOwned},
    verack::{VERACK_COMMAND, VerAck},
    version::{VERSION_COMMAND, VersionBorrowed, VersionOwned},
};

pub trait PacketPayload<'a, Owned: Serializable + From<Self>>:
    Clone + Debug + DeserializableBorrowed<'a>
{
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
) -> Result<Option<ReceivedPayload>> {
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
            let v = allocator.try_alloc_default::<VersionBorrowed>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::Version(v))
        }
        super::verack::VERACK_COMMAND => {
            let v = allocator.try_alloc_default::<VerAck>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::VerAck(v))
        }
        super::ping::PING_COMMAND => {
            let v = allocator.try_alloc_default::<Ping>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::Ping(v))
        }
        super::pong::PONG_COMMAND => {
            let v = allocator.try_alloc_default::<Pong>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::Pong(v))
        }
        super::sendaddrv2::SENDADDRV2_COMMAND => {
            let v = allocator.try_alloc_default::<SendAddrV2>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::SendAddrV2(v))
        }
        super::sendheaders::SENDHEADERS_COMMAND => {
            let v = allocator.try_alloc_default::<SendHeaders>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::SendHeaders(v))
        }
        super::tx::TX_COMMAND => {
            let v = allocator.try_alloc_default::<TxBorrowed>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::Tx(v))
        }
        super::block::BLOCK_COMMAND => {
            let v = allocator.try_alloc_default::<BlockBorrowed>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::Block(v))
        }
        super::addr::ADDR_COMMAND => {
            let v = allocator.try_alloc_default::<AddrBorrowed>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::Addr(v))
        }
        super::addrv2::ADDRV2_COMMAND => {
            let v = allocator.try_alloc_default::<AddrV2Borrowed>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::AddrV2(v))
        }
        super::inv::INV_COMMAND => {
            let v = allocator.try_alloc_default::<InvBorrowed>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::Inv(v))
        }
        super::headers::HEADERS_COMMAND => {
            let v = allocator.try_alloc_default::<HeadersBorrowed>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::Headers(v))
        }
        super::getheaders::GETHEADERS_COMMAND => {
            let v = allocator.try_alloc_default::<GetHeadersBorrowed>()?;
            v.deserialize_borrowed(allocator, buffer)?;
            Some(ReceivedPayload::GetHeaders(v))
        }
        _ => None,
    })
}

pub trait Serializable
where
    Self: Sized + Clone + Default,
{
    fn serialize(&self, stream: &mut impl BufMut);
}

pub trait DeserializableBorrowed<'bump> {
    // While the structs themselves are going to be allocated in the bump allocator,
    // the scripts and other byte-arrays are going to be referenced and not copied.
    fn deserialize_borrowed(
        &mut self,
        allocator: &'bump Arena,
        buffer: &'bump [u8],
    ) -> Result<usize>; // returned value is the length consumed
}

pub trait DeserializableOwned
where
    Self: Sized + Clone,
{
    fn deserialize_owned(buffer: &[u8]) -> Result<(Self, usize)>; // returned value is the length consumed
}

#[derive(Clone, Copy, Debug)]
pub enum ReceivedPayload<'a> {
    Ping(&'a Ping),
    Inv(&'a InvBorrowed<'a>),
    Version(&'a VersionBorrowed<'a>),
    VerAck(&'a VerAck),
    Addr(&'a AddrBorrowed<'a>),
    AddrV2(&'a AddrV2Borrowed<'a>),
    GetAddr(&'a GetAddr),
    Pong(&'a Pong),
    SendHeaders(&'a SendHeaders),
    SendAddrV2(&'a SendAddrV2),
    Tx(&'a TxBorrowed<'a>),
    Block(&'a BlockBorrowed<'a>),
    GetData(&'a GetDataBorrowed<'a>),
    Headers(&'a HeadersBorrowed<'a>),
    GetHeaders(&'a GetHeadersBorrowed<'a>),
}

#[derive(Clone, Debug)]
pub enum PayloadToSend {
    Ping(Ping),
    Inv(InvOwned),
    Version(VersionOwned),
    VerAck(VerAck),
    Addr(AddrOwned),
    AddrV2(AddrV2Owned),
    GetAddr(GetAddr),
    Pong(Pong),
    SendHeaders(SendHeaders),
    SendAddrV2(SendAddrV2),
    Tx(TxOwned),
    Block(BlockOwned),
    GetData(GetDataOwned),
    Headers(HeadersOwned),
    GetHeaders(GetHeadersOwned),
}

impl PayloadToSend {
    pub fn serialize(&self, stream: &mut std::vec::Vec<u8>) {
        match self {
            PayloadToSend::Version(version) => version.serialize(stream),
            PayloadToSend::VerAck(ver_ack) => ver_ack.serialize(stream),
            PayloadToSend::Ping(ping) => ping.serialize(stream),
            PayloadToSend::Pong(pong) => pong.serialize(stream),
            PayloadToSend::SendHeaders(send_headers) => send_headers.serialize(stream),
            PayloadToSend::SendAddrV2(send_addr_v2) => send_addr_v2.serialize(stream),
            PayloadToSend::Tx(tx) => tx.serialize(stream),
            PayloadToSend::Block(block) => block.serialize(stream),
            PayloadToSend::Addr(addr) => addr.serialize(stream),
            PayloadToSend::AddrV2(addrv2) => addrv2.serialize(stream),
            PayloadToSend::GetAddr(getaddr) => getaddr.serialize(stream),
            PayloadToSend::GetData(getdata) => getdata.serialize(stream),
            PayloadToSend::Inv(inv) => inv.serialize(stream),
            PayloadToSend::Headers(headers) => headers.serialize(stream),
            PayloadToSend::GetHeaders(getheaders) => getheaders.serialize(stream),
        }
    }

    pub fn command(&self) -> &'static [u8; 12] {
        match self {
            PayloadToSend::Version(_) => &VERSION_COMMAND,
            PayloadToSend::VerAck(_) => &VERACK_COMMAND,
            PayloadToSend::Ping(_) => &PING_COMMAND,
            PayloadToSend::Pong(_) => &PONG_COMMAND,
            PayloadToSend::SendHeaders(_) => &SENDHEADERS_COMMAND,
            PayloadToSend::SendAddrV2(_) => &SENDADDRV2_COMMAND,
            PayloadToSend::Tx(_) => &TX_COMMAND,
            PayloadToSend::Block(_) => &BLOCK_COMMAND,
            PayloadToSend::Addr(_) => &ADDR_COMMAND,
            PayloadToSend::AddrV2(_) => &ADDRV2_COMMAND,
            PayloadToSend::GetAddr(_) => &GETADDR_COMMAND,
            PayloadToSend::GetData(_) => &GETDATA_COMMAND,
            PayloadToSend::Inv(_) => &INV_COMMAND,
            PayloadToSend::Headers(_) => &HEADERS_COMMAND,
            PayloadToSend::GetHeaders(_) => &GETHEADERS_COMMAND,
        }
    }
}
