use anyhow::{Result, bail};
use bytes::BufMut;
use tokio::io::AsyncReadExt;

use super::{
    packetpayload::{Serializable, Stream},
    varstr::VarStr,
};

#[derive(Default, Clone, Copy)]
pub struct NetAddrShort {
    pub services: u64,
    pub addr: [u8; 16],
    pub port: u16,
}

impl Serializable<'_, '_> for NetAddrShort {
    async fn deserialize(
        &mut self,
        _allocator: &bumpalo::Bump<1>,
        stream: &mut impl Stream,
    ) -> Result<()> {
        self.services = stream.read_u64_le().await?;
        let read = stream.read_exact(&mut self.addr).await?;
        if read != 16 {
            bail!("could not read 16 bytes of address for netaddrshort")
        }
        self.port = stream.read_u16().await?;
        Ok(())
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u64_le(self.services);
        stream.put(self.addr.as_slice());
        stream.put_u16(self.port);
    }
}

#[derive(Default, Clone, Copy)]
pub struct NetAddr {
    services: u64,
    addr: [u8; 16],
    time: u32,
    port: u16,
}

impl Serializable<'_, '_> for NetAddr {
    async fn deserialize(
        &mut self,
        allocator: &'_ bumpalo::Bump<1>,
        stream: &mut impl Stream,
    ) -> Result<()> {
        self.time = stream.read_u32_le().await?;
        self.services = stream.read_u64_le().await?;
        stream.read_exact(&mut self.addr).await?;
        self.port = stream.read_u16().await?;
        Ok(())
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.time);
        stream.put_u64_le(self.services);
        stream.put(self.addr.as_slice());
        stream.put_u16(self.port);
    }
}

#[derive(Default, Clone)]
pub struct NetAddrV2<'a> {
    address: VarStr<'a>,
    services: u64,
    time: u32,
    port: u16,
    network_id: u8,
}

impl<'a, 'b> Serializable<'a, 'b> for NetAddrV2<'a> {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut impl Stream,
    ) -> Result<()> {
        self.time = stream.read_u32_le().await?;
        self.services.deserialize(allocator, stream).await?;
        self.network_id = stream.read_u8().await?;
        self.address.deserialize(allocator, stream).await?;
        self.port = stream.read_u16().await?;
        Ok(())
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_u32_le(self.time);
        self.services.serialize(stream);
        stream.put_u8(self.network_id);
        self.address.serialize(stream);
        stream.put_u16(self.port);
    }
}
