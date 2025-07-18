use super::{
    SupercowVec,
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable, SerializableValue},
    varint::VarInt,
};
use anyhow::bail;
use supercow::Supercow;

#[derive(Debug, Clone)]
pub struct GetHeaders<'a> {
    pub version: u32,
    pub block_locator: Supercow<'a, SupercowVec<'a, [u8; 32]>>,
    pub hash_stop: Supercow<'a, [u8; 32]>,
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
        let mut locator = Vec::with_capacity(self.block_locator.inner.len());
        for hash in self.block_locator.inner.iter() {
            locator.push(Supercow::owned(**hash));
        }
        Self::WithLifetime {
            version: self.version,
            hash_stop: Supercow::owned(*self.hash_stop),
            block_locator: Supercow::owned(SupercowVec {
                inner: Supercow::owned(locator),
            }),
        }
    }
}

impl<'a> Serializable<'a> for GetHeaders<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, Self>, usize)> {
        let version = buffer.get_u32_le(0)?;
        let (length, mut offset) = VarInt::deserialize(buffer.with_offset(4)?)?;
        offset += 4;
        let mut block_locator =
            bumpalo::collections::Vec::with_capacity_in(length as usize, allocator);
        for _ in 0..length as usize {
            let hash = buffer.get_hash(offset)?;
            block_locator.push(Supercow::borrowed(hash));
            offset += 32;
        }
        let hash_stop = buffer.get_hash(offset)?;
        match allocator.try_alloc(SupercowVec {
            inner: Supercow::borrowed(block_locator.into_bump_slice()),
        }) {
            Ok(v) => {
                match allocator.try_alloc(Self {
                    version,
                    block_locator: Supercow::borrowed(v),
                    hash_stop: Supercow::borrowed(hash_stop),
                }) {
                    Ok(result) => Ok((Supercow::borrowed(result), offset + 32)),
                    Err(e) => bail!(e),
                }
            }
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.version);
        let locator_length = self.block_locator.inner.len() as VarInt;
        locator_length.serialize(stream);
        for hash in self.block_locator.inner.iter() {
            stream.put_slice(hash.as_slice());
        }
        stream.put_slice(self.hash_stop.as_slice());
    }
}
