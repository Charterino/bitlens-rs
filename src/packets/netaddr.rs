use std::borrow::Cow;

use anyhow::{Result, anyhow, bail};
use bytes::BufMut;

use super::{
    buffer::Buffer,
    packetpayload::{Serializable, SerializableValue},
    varint::VarInt,
    varstr::VarStr,
};

const EMPTY_ADDR: &'static [u8; 16] = &[0u8; 16];

#[derive(Clone, Debug)]
pub struct NetAddrShort<'a> {
    pub services: u64,
    pub addr: Cow<'a, [u8; 16]>,
    pub port: u16,
}

impl<'a> Default for NetAddrShort<'a> {
    fn default() -> Self {
        Self {
            services: Default::default(),
            addr: Cow::Borrowed(EMPTY_ADDR),
            port: Default::default(),
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
                let res = allocator.alloc(NetAddrShort {
                    services: buffer.get_u64_le(0)?,
                    addr: Cow::Borrowed(b.try_into().unwrap()),
                    port: buffer.get_u16(24)?,
                });
                Ok((res, 26))
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

#[derive(Clone, Copy)]
pub struct NetAddr<'a> {
    pub services: u64,
    pub addr: &'a [u8; 16],
    pub time: u32,
    pub port: u16,
}

impl<'a> Serializable<'a> for NetAddr<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(&'a NetAddr<'a>, usize)> {
        let res = allocator.alloc(NetAddr {
            time: buffer.get_u32_le(0)?,
            services: buffer.get_u64_le(4)?,
            addr: buffer
                .get(12..28)
                .ok_or(anyhow!("not enough bytes for netaddr"))?
                .try_into()?,
            port: buffer.get_u16(28)?,
        });
        Ok((res, 30))
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.time);
        stream.put_u64_le(self.services);
        stream.put(self.addr.as_slice());
        stream.put_u16(self.port);
    }
}

#[derive(Clone)]
pub struct NetAddrV2<'a> {
    pub address: VarStr<'a>,
    pub services: VarInt,
    pub time: u32,
    pub port: u16,
    pub network_id: u8,
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
        offset += 1;
        let (address, offset_delta) = VarStr::deserialize(allocator, buffer.with_offset(offset)?)?;
        offset += offset_delta;
        let port = buffer.get_u16(offset)?;
        let res = allocator.alloc(NetAddrV2 {
            address,
            services,
            time,
            port,
            network_id,
        });
        Ok((res, offset + 2))
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.time);
        self.services.serialize(stream);
        stream.put_u8(self.network_id);
        self.address.serialize(stream);
        stream.put_u16(self.port);
    }
}
