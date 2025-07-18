use super::{
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    network_id::NetworkId,
    packetpayload::{Serializable, SerializableValue},
    varint::VarInt,
    varstr::VarStr,
};
use anyhow::{Result, anyhow, bail};
use bytes::BufMut;
use num::{FromPrimitive, ToPrimitive};
use supercow::Supercow;

#[derive(Clone, Debug)]
pub struct NetAddrShort<'a> {
    pub services: u64,
    pub addr: Supercow<'a, [u8; 16]>,
    pub port: u16,
}

impl<'old> MustOutlive<'old> for NetAddrShort<'old> {
    type WithLifetime<'new: 'old> = NetAddrShort<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for NetAddrShort<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let addr = *self.addr;
        Self::WithLifetime {
            services: self.services,
            addr: Supercow::owned(addr),
            port: self.port,
        }
    }
}

impl<'a> Serializable<'a> for NetAddrShort<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(Supercow<'a, NetAddrShort<'a>>, usize)> {
        match buffer.get(8..24) {
            Some(b) => {
                let address: &[u8; 16] = b.try_into().unwrap();
                match allocator.try_alloc(NetAddrShort {
                    services: buffer.get_u64_le(0)?,
                    addr: Supercow::borrowed(address),
                    port: buffer.get_u16(24)?,
                }) {
                    Ok(result) => Ok((Supercow::borrowed(result), 26)),
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
    pub addr: Supercow<'a, [u8; 16]>,
    pub time: u32,
    pub port: u16,
}

impl<'old> MustOutlive<'old> for NetAddr<'old> {
    type WithLifetime<'new: 'old> = NetAddr<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for NetAddr<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let addr = *self.addr;
        Self::WithLifetime {
            services: self.services,
            addr: Supercow::owned(addr),
            time: self.time,
            port: self.port,
        }
    }
}

impl<'a> Serializable<'a> for NetAddr<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(Supercow<'a, NetAddr<'a>>, usize)> {
        match buffer.get(12..28) {
            Some(address) => {
                let address: &[u8; 16] = address.try_into().unwrap();
                match allocator.try_alloc(NetAddr {
                    time: buffer.get_u32_le(0)?,
                    services: buffer.get_u64_le(4)?,
                    addr: Supercow::borrowed(address),
                    port: buffer.get_u16(28)?,
                }) {
                    Ok(result) => Ok((Supercow::borrowed(result), 30)),
                    Err(e) => bail!(e),
                }
            }
            None => bail!("not enough bytes for netaddr"),
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
    pub address: Supercow<'a, VarStr<'a>>,
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
            address: Supercow::owned(self.address.deep_clone()),
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
    ) -> Result<(Supercow<'a, NetAddrV2<'a>>, usize)> {
        let time = buffer.get_u32_le(0)?;
        let (services, mut offset) = VarInt::deserialize(buffer.with_offset(4)?)?;
        offset += 4;
        let network_id = *buffer
            .get(offset)
            .ok_or(anyhow!("not enough bytes for netaddrv2"))?;
        let network_id = NetworkId::from_u8(network_id).ok_or(anyhow!("invalid network_id"))?;
        offset += 1;
        let (address, offset_delta) =
            <VarStr as Serializable>::deserialize(allocator, buffer.with_offset(offset)?)?;
        offset += offset_delta;
        let port = buffer.get_u16(offset)?;
        match allocator.try_alloc(NetAddrV2 {
            address,
            services,
            time,
            port,
            network_id,
        }) {
            Ok(result) => Ok((Supercow::borrowed(result), offset + 2)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.time);
        self.services.serialize(stream);
        stream.put_u8(self.network_id.to_u8().unwrap());
        Serializable::serialize(&*self.address, stream);
        stream.put_u16(self.port);
    }
}
