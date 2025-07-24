use crate::util::arena::Arena;

use super::{
    blockheader::{BlockHeaderBorrowed, BlockHeaderOwned},
    deserialize_array,
    packetpayload::{DeserializableBorrowed, PacketPayload, Serializable},
    serialize_array,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct HeadersBorrowed<'a> {
    pub inner: &'a [BlockHeaderBorrowed<'a>],
}

pub const HEADERS_COMMAND: [u8; 12] = *b"headers\0\0\0\0\0";

impl<'a> PacketPayload<'a, HeadersOwned> for HeadersBorrowed<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &HEADERS_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for HeadersBorrowed<'a> {
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
pub struct HeadersOwned {
    pub inner: Vec<BlockHeaderOwned>,
}

impl Serializable for HeadersOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        serialize_array(&self.inner, stream);
    }
}

impl From<HeadersBorrowed<'_>> for HeadersOwned {
    fn from(value: HeadersBorrowed<'_>) -> Self {
        Self {
            inner: value.inner.iter().map(|v| (*v).into()).collect(),
        }
    }
}
