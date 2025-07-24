use crate::util::arena::Arena;

use super::{
    EMPTY_HASH,
    buffer::Buffer,
    packetpayload::{DeserializableBorrowed, PacketPayload, Serializable},
    varint::{VarInt, deserialize_varint, serialize_varint},
};

#[derive(Debug, Clone, Copy)]
pub struct GetHeadersBorrowed<'a> {
    pub version: u32,
    pub block_locator: &'a [&'a [u8; 32]],
    pub hash_stop: &'a [u8; 32],
}

impl Default for GetHeadersBorrowed<'_> {
    fn default() -> Self {
        Self {
            version: Default::default(),
            block_locator: Default::default(),
            hash_stop: &EMPTY_HASH,
        }
    }
}

pub const GETHEADERS_COMMAND: [u8; 12] = *b"getheaders\0\0";

impl<'a> PacketPayload<'a, GetHeadersOwned> for GetHeadersBorrowed<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &GETHEADERS_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for GetHeadersBorrowed<'a> {
    fn deserialize_borrowed(
        &mut self,
        allocator: &'a Arena,
        buffer: &'a [u8],
    ) -> anyhow::Result<usize> {
        self.version = buffer.get_u32_le(0)?;
        let (length, mut offset) = deserialize_varint(buffer.with_offset(4)?)?;
        offset += 4;
        let mut block_locator = allocator.try_alloc_arenaarray(length as usize)?;
        for _ in 0..length as usize {
            let hash = buffer.get_hash(offset)?;
            block_locator.push(hash);
            offset += 32;
        }
        self.hash_stop = buffer.get_hash(offset)?;
        self.block_locator = block_locator.into_arena_array();
        Ok(offset + 32)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GetHeadersOwned {
    pub version: u32,
    pub block_locator: Vec<[u8; 32]>,
    pub hash_stop: [u8; 32],
}

impl Serializable for GetHeadersOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.version);
        serialize_varint(self.block_locator.len() as VarInt, stream);
        for hash in &self.block_locator {
            stream.put_slice(hash.as_slice());
        }
        stream.put_slice(self.hash_stop.as_slice());
    }
}

impl From<GetHeadersBorrowed<'_>> for GetHeadersOwned {
    fn from(value: GetHeadersBorrowed<'_>) -> Self {
        Self {
            version: value.version,
            block_locator: value.block_locator.iter().map(|v| **v).collect(),
            hash_stop: *value.hash_stop,
        }
    }
}
