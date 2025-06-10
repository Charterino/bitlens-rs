use std::borrow::Cow;

use super::buffer::Buffer;
use super::packetpayload::{Serializable, SerializableValue};
use super::varstr::VarStr;
use super::{netaddr::NetAddrShort, packetpayload::PacketPayload};
use anyhow::{Result, anyhow};
use bytes::BufMut;

#[derive(Debug, Clone, Default)]
pub struct Version<'a> {
    pub services: u64,
    pub timestamp: u64,
    pub addrrecv: Cow<'a, NetAddrShort<'a>>,
    pub addrfrom: Cow<'a, NetAddrShort<'a>>,
    pub nonce: u64,
    pub user_agent: VarStr<'a>,
    pub start_height: u32,
    pub version: u32,
    pub announce_relayed_transactions: bool,
}

pub const VERSION_COMMAND: [u8; 12] = *b"version\0\0\0\0\0";

impl<'a> PacketPayload<'a> for Version<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &VERSION_COMMAND
    }
}

impl<'a> Serializable<'a> for Version<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(&'a Version<'a>, usize)> {
        let version = buffer.get_u32_le(0)?;
        let services = buffer.get_u64_le(4)?;
        let timestamp = buffer.get_u64_le(12)?;
        let (addrrecv, _) = NetAddrShort::deserialize(allocator, buffer.with_offset(20)?)?;
        let (addrfrom, _) = NetAddrShort::deserialize(allocator, buffer.with_offset(46)?)?;
        let nonce = buffer.get_u64_le(72)?;
        let (ua, offset) = VarStr::deserialize(allocator, buffer.with_offset(80)?)?;
        let start_height = buffer.get_u32_le(80 + offset)?;
        let res = allocator.alloc(Version {
            services,
            timestamp,
            addrrecv: Cow::Borrowed(addrrecv),
            addrfrom: Cow::Borrowed(addrfrom),
            nonce,
            user_agent: ua,
            start_height,
            version,
            announce_relayed_transactions: false,
        });
        if version >= 70001 {
            res.announce_relayed_transactions = *buffer
                .get(84 + offset)
                .ok_or(anyhow!("missing last byte in version"))?
                != 0;
            return Ok((res, offset + 85));
        }
        Ok((res, offset + 84))
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.version);
        stream.put_u64_le(self.services);
        stream.put_u64_le(self.timestamp);
        self.addrrecv.serialize(stream);
        self.addrfrom.serialize(stream);
        stream.put_u64_le(self.nonce);
        self.user_agent.serialize(stream);
        stream.put_u32_le(self.start_height);
        if self.version >= 70001 {
            stream.put_u8(self.announce_relayed_transactions as u8);
        }
    }
}
