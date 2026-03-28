use super::{
    deserialize_array,
    invvector::{InventoryVectorBorrowed, InventoryVectorOwned},
    packetpayload::{DeserializableBorrowed, PacketPayload, Serializable},
    serialize_array,
};
use crate::util::arena::Arena;

#[derive(Debug, Clone, Copy, Default)]
pub struct NotFoundBorrowed<'a> {
    pub inner: &'a [InventoryVectorBorrowed<'a>],
}

pub const NOTFOUND_COMMAND: [u8; 12] = *b"notfound\0\0\0\0";

impl<'a> PacketPayload<'a, NotFoundOwned> for NotFoundBorrowed<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &NOTFOUND_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for NotFoundBorrowed<'a> {
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
pub struct NotFoundOwned {
    pub inner: Vec<InventoryVectorOwned>,
}

impl Serializable for NotFoundOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        serialize_array(&self.inner, stream);
    }
}

impl From<NotFoundBorrowed<'_>> for NotFoundOwned {
    fn from(value: NotFoundBorrowed<'_>) -> Self {
        Self {
            inner: value.inner.iter().map(|v| (*v).into()).collect(),
        }
    }
}
