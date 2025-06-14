use std::borrow::Cow;

use anyhow::{Result, anyhow, bail};
use bytes::BufMut;
use num::{FromPrimitive, ToPrimitive};

use crate::types::network_id::NetworkId;

use super::{
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{Serializable, SerializableValue},
    varint::VarInt,
    varstr::VarStr,
};

#[derive(Clone, Debug, Default)]
pub struct NetAddrShort<'a> {
    pub services: u64,
    pub addr: Cow<'a, [u8; 16]>,
    pub port: u16,
}

impl<'old> MustOutlive<'old> for NetAddrShort<'old> {
    type WithLifetime<'new: 'old> = NetAddrShort<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for NetAddrShort<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let addr = self.addr.clone().into_owned();
        Self::WithLifetime {
            services: self.services,
            addr: Cow::Owned(addr),
            port: self.port,
        }
    }
}

impl<'a> Serializable<'a> for NetAddrShort<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(&'a NetAddrShort<'a>, usize)> {
        match buffer.get(8..24) {
            Some(b) => {
                match allocator.try_alloc(NetAddrShort {
                    services: buffer.get_u64_le(0)?,
                    addr: Cow::Borrowed(b.try_into().unwrap()),
                    port: buffer.get_u16(24)?,
                }) {
                    Ok(result) => Ok((result, 26)),
                    Err(e) => bail!(e),
                }
            }
            None => bail!("not enough bytes for netaddrshort"),
        }
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u64_le(self.services);
        stream.put(self.addr.as_slice());
        stream.put_u16(self.port);
    }
}

#[derive(Clone, Debug)]
pub struct NetAddr<'a> {
    pub services: u64,
    pub addr: Cow<'a, [u8; 16]>,
    pub time: u32,
    pub port: u16,
}

impl<'old> MustOutlive<'old> for NetAddr<'old> {
    type WithLifetime<'new: 'old> = NetAddr<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for NetAddr<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let addr = self.addr.clone().into_owned();
        Self::WithLifetime {
            services: self.services,
            addr: Cow::Owned(addr),
            time: self.time,
            port: self.port,
        }
    }
}

impl<'a> Serializable<'a> for NetAddr<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(&'a NetAddr<'a>, usize)> {
        match allocator.try_alloc(NetAddr {
            time: buffer.get_u32_le(0)?,
            services: buffer.get_u64_le(4)?,
            addr: Cow::Borrowed(
                buffer
                    .get(12..28)
                    .ok_or(anyhow!("not enough bytes for netaddr"))?
                    .try_into()?,
            ),
            port: buffer.get_u16(28)?,
        }) {
            Ok(result) => Ok((result, 30)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.time);
        stream.put_u64_le(self.services);
        stream.put(self.addr.as_slice());
        stream.put_u16(self.port);
    }
}

#[derive(Clone, Debug)]
pub struct NetAddrV2<'a> {
    pub address: VarStr<'a>,
    pub services: VarInt,
    pub time: u32,
    pub port: u16,
    pub network_id: NetworkId,
}

impl<'old> MustOutlive<'old> for NetAddrV2<'old> {
    type WithLifetime<'new: 'old> = NetAddrV2<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for NetAddrV2<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        Self::WithLifetime {
            address: self.address.deep_clone(),
            services: self.services,
            time: self.time,
            port: self.port,
            network_id: self.network_id,
        }
    }
}

impl<'a> Serializable<'a> for NetAddrV2<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(&'a NetAddrV2<'a>, usize)> {
        let time = buffer.get_u32_le(0)?;
        let (services, mut offset) = VarInt::deserialize(allocator, buffer.with_offset(4)?)?;
        offset += 4;
        let network_id = *buffer
            .get(offset)
            .ok_or(anyhow!("not enough bytes for netaddrv2"))?;
        let network_id = NetworkId::from_u8(network_id).ok_or(anyhow!("invalid network_id"))?;
        offset += 1;
        let (address, offset_delta) = VarStr::deserialize(allocator, buffer.with_offset(offset)?)?;
        offset += offset_delta;
        let port = buffer.get_u16(offset)?;
        match allocator.try_alloc(NetAddrV2 {
            address,
            services,
            time,
            port,
            network_id,
        }) {
            Ok(result) => Ok((result, offset + 2)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.time);
        self.services.serialize(stream);
        stream.put_u8(self.network_id.to_u8().unwrap());
        self.address.serialize(stream);
        stream.put_u16(self.port);
    }
}
