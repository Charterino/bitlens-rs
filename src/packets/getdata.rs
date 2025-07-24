use crate::util::arena::Arena;

use super::{
    deserialize_array,
    invvector::{InventoryVectorBorrowed, InventoryVectorOwned},
    packetpayload::{DeserializableBorrowed, PacketPayload, Serializable},
    serialize_array,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct GetDataBorrowed<'a> {
    pub inner: &'a [InventoryVectorBorrowed<'a>],
}

pub const GETDATA_COMMAND: [u8; 12] = *b"getdata\0\0\0\0\0";

impl<'a> PacketPayload<'a, GetDataOwned> for GetDataBorrowed<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &GETDATA_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for GetDataBorrowed<'a> {
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
pub struct GetDataOwned {
    pub inner: Vec<InventoryVectorOwned>,
}

impl Serializable for GetDataOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        serialize_array(&self.inner, stream);
    }
}

impl From<GetDataBorrowed<'_>> for GetDataOwned {
    fn from(value: GetDataBorrowed<'_>) -> Self {
        Self {
            inner: value.inner.iter().map(|v| (*v).into()).collect(),
        }
    }
}
