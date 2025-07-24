use crate::util::arena::Arena;

use super::{
    deserialize_array,
    invvector::{InventoryVectorBorrowed, InventoryVectorOwned},
    packetpayload::{DeserializableBorrowed, PacketPayload, Serializable},
    serialize_array,
};

#[derive(Clone, Debug, Copy, Default)]
pub struct InvBorrowed<'a> {
    pub inner: &'a [InventoryVectorBorrowed<'a>],
}

pub const INV_COMMAND: [u8; 12] = *b"inv\0\0\0\0\0\0\0\0\0";

impl<'a> PacketPayload<'a, InvOwned> for InvBorrowed<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &INV_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for InvBorrowed<'a> {
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

#[derive(Clone, Debug, Default)]
pub struct InvOwned {
    pub inner: Vec<InventoryVectorOwned>,
}

impl Serializable for InvOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        serialize_array(&self.inner, stream);
    }
}

impl From<InvBorrowed<'_>> for InvOwned {
    fn from(value: InvBorrowed<'_>) -> Self {
        Self {
            inner: value.inner.iter().map(|v| (*v).into()).collect(),
        }
    }
}
