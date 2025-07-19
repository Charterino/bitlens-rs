use crate::util::arena::Arena;

use super::buffer::Buffer;
use super::deepclone::{DeepClone, MustOutlive};
use super::packetpayload::Serializable;
use super::varstr::VarStr;
use super::{netaddr::NetAddrShort, packetpayload::PacketPayload};
use anyhow::{Result, anyhow, bail};
use bytes::BufMut;
use supercow::Supercow;

#[derive(Debug, Clone)]
pub struct Version<'a> {
    pub services: u64,
    pub timestamp: u64,
    pub addrrecv: Supercow<'a, NetAddrShort<'a>>,
    pub addrfrom: Supercow<'a, NetAddrShort<'a>>,
    pub nonce: u64,
    pub user_agent: Supercow<'a, VarStr<'a>>,
    pub start_height: u32,
    pub version: u32,
    pub announce_relayed_transactions: bool,
}

pub const VERSION_COMMAND: [u8; 12] = *b"version\0\0\0\0\0";

impl<'old> MustOutlive<'old> for Version<'old> {
    type WithLifetime<'new: 'old> = Version<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for Version<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        Self::WithLifetime {
            services: self.services,
            timestamp: self.timestamp,
            addrrecv: Supercow::owned(self.addrrecv.deep_clone()),
            addrfrom: Supercow::owned(self.addrfrom.deep_clone()),
            nonce: self.nonce,
            user_agent: Supercow::owned(self.user_agent.deep_clone()),
            start_height: self.start_height,
            version: self.version,
            announce_relayed_transactions: self.announce_relayed_transactions,
        }
    }
}

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for Version<'old> {
    fn command(&self) -> &'static [u8; 12] {
        &VERSION_COMMAND
    }
}

impl<'a> Serializable<'a> for Version<'a> {
    fn deserialize(
        allocator: &'a Arena,
        buffer: &'a [u8],
    ) -> Result<(Supercow<'a, Version<'a>>, usize)> {
        let version = buffer.get_u32_le(0)?;
        let services = buffer.get_u64_le(4)?;
        let timestamp = buffer.get_u64_le(12)?;
        let (addrrecv, _) = NetAddrShort::deserialize(allocator, buffer.with_offset(20)?)?;
        let (addrfrom, _) = NetAddrShort::deserialize(allocator, buffer.with_offset(46)?)?;
        let nonce = buffer.get_u64_le(72)?;
        let (ua, offset) =
            <VarStr as Serializable>::deserialize(allocator, buffer.with_offset(80)?)?;
        let start_height = buffer.get_u32_le(80 + offset)?;
        match allocator.try_alloc(Version {
            services,
            timestamp,
            addrrecv,
            addrfrom,
            nonce,
            user_agent: ua,
            start_height,
            version,
            announce_relayed_transactions: false,
        }) {
            Ok(result) => {
                if version >= 70001 {
                    result.announce_relayed_transactions = *buffer
                        .get(84 + offset)
                        .ok_or(anyhow!("missing last byte in version"))?
                        != 0;
                    return Ok((Supercow::borrowed(result), offset + 85));
                }
                Ok((Supercow::borrowed(result), offset + 84))
            }
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.version);
        stream.put_u64_le(self.services);
        stream.put_u64_le(self.timestamp);
        self.addrrecv.serialize(stream);
        self.addrfrom.serialize(stream);
        stream.put_u64_le(self.nonce);
        Serializable::serialize(&*self.user_agent, stream);
        stream.put_u32_le(self.start_height);
        if self.version >= 70001 {
            stream.put_u8(self.announce_relayed_transactions as u8);
        }
    }
}
