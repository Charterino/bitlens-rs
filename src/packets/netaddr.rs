use crate::util::arena::Arena;

use super::{
    buffer::Buffer,
    network_id::NetworkId,
    packetpayload::{DeserializableBorrowed, Serializable},
    varint::{VarInt, deserialize_varint, serialize_varint},
    varstr::{deserialize_varstr, serialize_varstr},
};
use anyhow::{Result, anyhow, bail};
use bytes::BufMut;
use num::{FromPrimitive, ToPrimitive};

pub const EMPTY_ADDR: [u8; 16] = [0u8; 16];

#[derive(Clone, Copy, Debug)]
pub struct NetAddrShortBorrowed<'a> {
    pub services: u64,
    pub addr: &'a [u8; 16],
    pub port: u16,
}

impl Default for NetAddrShortBorrowed<'_> {
    fn default() -> Self {
        Self {
            services: Default::default(),
            addr: &EMPTY_ADDR,
            port: Default::default(),
        }
    }
}

impl<'a> DeserializableBorrowed<'a> for NetAddrShortBorrowed<'a> {
    fn deserialize_borrowed(&mut self, _: &Arena, buffer: &'a [u8]) -> Result<usize> {
        match buffer.get(8..24) {
            Some(b) => {
                self.services = buffer.get_u64_le(0)?;
                self.addr = b.try_into().unwrap();
                self.port = buffer.get_u16(24)?;
                Ok(26)
            }
            None => bail!("not enough bytes for netaddrshort"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NetAddrShortOwned {
    pub services: u64,
    pub addr: [u8; 16],
    pub port: u16,
}

impl Serializable for NetAddrShortOwned {
    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u64_le(self.services);
        stream.put(self.addr.as_slice());
        stream.put_u16(self.port);
    }
}

impl From<NetAddrShortBorrowed<'_>> for NetAddrShortOwned {
    fn from(value: NetAddrShortBorrowed<'_>) -> Self {
        NetAddrShortOwned {
            services: value.services,
            addr: *value.addr,
            port: value.port,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NetAddrBorrowed<'a> {
    pub services: u64,
    pub addr: &'a [u8; 16],
    pub time: u32,
    pub port: u16,
}

impl Default for NetAddrBorrowed<'_> {
    fn default() -> Self {
        Self {
            services: Default::default(),
            addr: &EMPTY_ADDR,
            time: Default::default(),
            port: Default::default(),
        }
    }
}

impl<'a> DeserializableBorrowed<'a> for NetAddrBorrowed<'a> {
    fn deserialize_borrowed(&mut self, _: &Arena, buffer: &'a [u8]) -> Result<usize> {
        match buffer.get(12..28) {
            Some(address) => {
                self.time = buffer.get_u32_le(0)?;
                self.addr = address.try_into().unwrap();
                self.services = buffer.get_u64_le(4)?;
                self.port = buffer.get_u16(28)?;
                Ok(30)
            }
            None => bail!("not enough bytes for netaddr"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NetAddrOwned {
    pub services: u64,
    pub addr: [u8; 16],
    pub time: u32,
    pub port: u16,
}

impl Serializable for NetAddrOwned {
    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.time);
        stream.put_u64_le(self.services);
        stream.put(self.addr.as_slice());
        stream.put_u16(self.port);
    }
}

impl From<NetAddrBorrowed<'_>> for NetAddrOwned {
    fn from(value: NetAddrBorrowed<'_>) -> Self {
        NetAddrOwned {
            services: value.services,
            addr: *value.addr,
            time: value.time,
            port: value.port,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NetAddrV2Borrowed<'a> {
    pub address: &'a [u8],
    pub services: VarInt,
    pub time: u32,
    pub port: u16,
    pub network_id: NetworkId,
}

impl<'a> DeserializableBorrowed<'a> for NetAddrV2Borrowed<'a> {
    fn deserialize_borrowed(&mut self, _: &Arena, buffer: &'a [u8]) -> Result<usize> {
        self.time = buffer.get_u32_le(0)?;
        let (services, mut offset) = deserialize_varint(buffer.with_offset(4)?)?;
        self.services = services;
        offset += 4;
        let network_id = *buffer
            .get(offset)
            .ok_or(anyhow!("not enough bytes for netaddrv2"))?;
        self.network_id = NetworkId::from_u8(network_id).ok_or(anyhow!("invalid network_id"))?;
        offset += 1;
        let (address, offset_delta) = deserialize_varstr(buffer.with_offset(offset)?)?;
        self.address = address;
        offset += offset_delta;
        self.port = buffer.get_u16(offset)?;

        Ok(offset + 2)
    }
}

#[derive(Clone, Debug, Default)]
pub struct NetAddrV2Owned {
    pub address: Vec<u8>,
    pub services: VarInt,
    pub time: u32,
    pub port: u16,
    pub network_id: NetworkId,
}

impl Serializable for NetAddrV2Owned {
    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.time);
        serialize_varint(self.services, stream);
        stream.put_u8(self.network_id.to_u8().unwrap());
        serialize_varstr(&self.address, stream);
        stream.put_u16(self.port);
    }
}

impl From<NetAddrV2Borrowed<'_>> for NetAddrV2Owned {
    fn from(value: NetAddrV2Borrowed<'_>) -> Self {
        NetAddrV2Owned {
            address: value.address.to_vec(),
            services: value.services,
            time: value.time,
            port: value.port,
            network_id: value.network_id,
        }
    }
}
