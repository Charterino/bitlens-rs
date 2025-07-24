use crate::util::arena::Arena;

use super::{
    deserialize_array,
    netaddr::{NetAddrBorrowed, NetAddrOwned},
    packetpayload::{DeserializableBorrowed, PacketPayload, Serializable},
    serialize_array,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct AddrBorrowed<'a> {
    pub inner: &'a [NetAddrBorrowed<'a>],
}

pub const ADDR_COMMAND: [u8; 12] = *b"addr\0\0\0\0\0\0\0\0";

impl<'a> PacketPayload<'a, AddrOwned> for AddrBorrowed<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &ADDR_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for AddrBorrowed<'a> {
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
pub struct AddrOwned {
    pub inner: Vec<NetAddrOwned>,
}

impl Serializable for AddrOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        serialize_array(&self.inner, stream);
    }
}

impl From<AddrBorrowed<'_>> for AddrOwned {
    fn from(value: AddrBorrowed<'_>) -> Self {
        Self {
            inner: value.inner.iter().map(|v| (*v).into()).collect(),
        }
    }
}
