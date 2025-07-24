use crate::util::arena::Arena;

use super::{
    deserialize_array,
    netaddr::{NetAddrV2Borrowed, NetAddrV2Owned},
    packetpayload::{DeserializableBorrowed, PacketPayload, Serializable},
    serialize_array,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct AddrV2Borrowed<'a> {
    pub inner: &'a [NetAddrV2Borrowed<'a>],
}

pub const ADDRV2_COMMAND: [u8; 12] = *b"addrv2\0\0\0\0\0\0";

impl<'a> PacketPayload<'a, AddrV2Owned> for AddrV2Borrowed<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &ADDRV2_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for AddrV2Borrowed<'a> {
    fn deserialize_borrowed(
        &mut self,
        allocator: &'a Arena,
        buffer: &'a [u8],
    ) -> anyhow::Result<usize> {
        let (deserialized, consumed) = deserialize_array(allocator, buffer)?;
        self.inner = deserialized;
        Ok(consumed)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AddrV2Owned {
    pub inner: Vec<NetAddrV2Owned>,
}

impl Serializable for AddrV2Owned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        serialize_array(&self.inner, stream);
    }
}

impl From<AddrV2Borrowed<'_>> for AddrV2Owned {
    fn from(value: AddrV2Borrowed<'_>) -> Self {
        Self {
            inner: value.inner.iter().map(|v| (*v).into()).collect(),
        }
    }
}
