use std::borrow::Cow;

use anyhow::bail;

use super::{
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable, SerializableValue},
    varint::VarInt,
};

#[derive(Debug, Clone, Default)]
pub struct GetHeaders<'a> {
    pub version: u32,
    pub block_locator: Cow<'a, [Cow<'a, [u8; 32]>]>,
    pub hash_stop: Cow<'a, [u8; 32]>,
}

pub const GETHEADERS_COMMAND: [u8; 12] = *b"getheaders\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for GetHeaders<'old> {
    fn command(&self) -> &'static [u8; 12] {
        &GETHEADERS_COMMAND
    }
}

impl<'old> MustOutlive<'old> for GetHeaders<'old> {
    type WithLifetime<'new: 'old> = GetHeaders<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for GetHeaders<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let mut locator = Vec::with_capacity(self.block_locator.len());
        for hash in self.block_locator.iter() {
            locator.push(Cow::Owned(hash.clone().into_owned()));
        }
        Self::WithLifetime {
            version: self.version,
            hash_stop: Cow::Owned(self.hash_stop.clone().into_owned()),
            block_locator: Cow::Owned(locator),
        }
    }
}

impl<'a> Serializable<'a> for GetHeaders<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a Self, usize)> {
        let version = buffer.get_u32_le(0)?;
        let (length, mut offset) = VarInt::deserialize(allocator, buffer.with_offset(4)?)?;
        offset += 4;
        let mut block_locator = Vec::with_capacity(length as usize);
        for _ in 0..length as usize {
            let hash = buffer.get_hash(offset)?;
            block_locator.push(Cow::Borrowed(hash));
            offset += 32;
        }
        let hash_stop = buffer.get_hash(offset)?;
        match allocator.try_alloc(Self {
            version,
            block_locator: Cow::Owned(block_locator),
            hash_stop: Cow::Borrowed(hash_stop),
        }) {
            Ok(result) => Ok((result, offset + 32)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.version);
        let locator_length = self.block_locator.len() as VarInt;
        locator_length.serialize(stream);
        for hash in self.block_locator.iter() {
            stream.put_slice(hash.as_slice());
        }
        stream.put_slice(self.hash_stop.as_slice());
    }
}
