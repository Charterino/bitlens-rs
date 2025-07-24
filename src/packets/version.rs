use crate::util::arena::Arena;

use super::netaddr::{NetAddrShortBorrowed, NetAddrShortOwned};
use super::packetpayload::{DeserializableBorrowed, Serializable};
use super::varstr::{deserialize_varstr, serialize_varstr};
use super::{buffer::Buffer, packetpayload::PacketPayload};
use anyhow::Result;
use bytes::BufMut;

#[derive(Debug, Clone, Copy, Default)]
pub struct VersionBorrowed<'a> {
    pub services: u64,
    pub timestamp: u64,
    pub addrrecv: NetAddrShortBorrowed<'a>,
    pub addrfrom: NetAddrShortBorrowed<'a>,
    pub nonce: u64,
    pub user_agent: &'a [u8],
    pub start_height: u32,
    pub version: u32,
    pub announce_relayed_transactions: bool,
}

pub const VERSION_COMMAND: [u8; 12] = *b"version\0\0\0\0\0";

impl<'a> PacketPayload<'a, VersionOwned> for VersionBorrowed<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &VERSION_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for VersionBorrowed<'a> {
    fn deserialize_borrowed(&mut self, allocator: &'a Arena, buffer: &'a [u8]) -> Result<usize> {
        self.version = buffer.get_u32_le(0)?;
        self.services = buffer.get_u64_le(4)?;
        self.timestamp = buffer.get_u64_le(12)?;
        self.addrrecv
            .deserialize_borrowed(allocator, buffer.with_offset(20)?)?;
        self.addrfrom
            .deserialize_borrowed(allocator, buffer.with_offset(46)?)?;
        self.nonce = buffer.get_u64_le(72)?;
        let (ua, offset) = deserialize_varstr(buffer.with_offset(80)?)?;
        self.user_agent = ua;
        self.start_height = buffer.get_u32_le(80 + offset)?;
        if self.version >= 70001 {
            if let Some(b) = buffer.get(84 + offset) {
                self.announce_relayed_transactions = *b != 0;
                return Ok(offset + 85);
            }
        }
        Ok(offset + 84)
    }
}

#[derive(Debug, Clone, Default)]
pub struct VersionOwned {
    pub services: u64,
    pub timestamp: u64,
    pub addrrecv: NetAddrShortOwned,
    pub addrfrom: NetAddrShortOwned,
    pub nonce: u64,
    pub user_agent: Vec<u8>,
    pub start_height: u32,
    pub version: u32,
    pub announce_relayed_transactions: bool,
}

impl Serializable for VersionOwned {
    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.version);
        stream.put_u64_le(self.services);
        stream.put_u64_le(self.timestamp);
        self.addrrecv.serialize(stream);
        self.addrfrom.serialize(stream);
        stream.put_u64_le(self.nonce);
        serialize_varstr(&self.user_agent, stream);
        stream.put_u32_le(self.start_height);
        if self.version >= 70001 {
            stream.put_u8(self.announce_relayed_transactions as u8);
        }
    }
}

impl From<VersionBorrowed<'_>> for VersionOwned {
    fn from(value: VersionBorrowed<'_>) -> Self {
        VersionOwned {
            services: value.services,
            timestamp: value.timestamp,
            addrrecv: value.addrrecv.into(),
            addrfrom: value.addrfrom.into(),
            nonce: value.nonce,
            user_agent: value.user_agent.to_vec(),
            start_height: value.start_height,
            version: value.version,
            announce_relayed_transactions: value.announce_relayed_transactions,
        }
    }
}
