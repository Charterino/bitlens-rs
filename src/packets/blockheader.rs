use std::borrow::Cow;

use sha2::{Digest, Sha256};

use super::{
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::Serializable,
};

#[derive(Clone, Debug, Default)]
pub struct BlockHeader<'a> {
    pub version: u32,
    parent: Cow<'a, [u8; 32]>,
    merkle_root: Cow<'a, [u8; 32]>,
    timestamp: u32,
    bits: u32,
    nonce: u32,

    hash: [u8; 32], // calculated during deserialization
}

impl<'old> MustOutlive<'old> for BlockHeader<'old> {
    type WithLifetime<'new: 'old> = BlockHeader<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for BlockHeader<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        Self::WithLifetime {
            version: self.version,
            parent: Cow::Owned(self.parent.clone().into_owned()),
            merkle_root: Cow::Owned(self.merkle_root.clone().into_owned()),
            timestamp: self.timestamp,
            bits: self.bits,
            nonce: self.nonce,
            hash: self.hash,
        }
    }
}

impl<'a> Serializable<'a> for BlockHeader<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a Self, usize)> {
        let version = buffer.get_u32_le(0)?;
        let parent = Cow::Borrowed(buffer.get_hash(4)?);
        let merkle_root = Cow::Borrowed(buffer.get_hash(36)?);
        let timestamp = buffer.get_u32_le(68)?;
        let bits = buffer.get_u32_le(72)?;
        let nonce = buffer.get_u32_le(76)?;

        let hash = Sha256::digest(buffer.get(0..80).unwrap());
        let hash = Sha256::digest(hash).into();

        Ok((
            allocator.alloc(Self {
                version,
                parent,
                merkle_root,
                timestamp,
                bits,
                nonce,
                hash,
            }),
            80,
        ))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.version);
        stream.put_slice(self.parent.as_slice());
        stream.put_slice(self.merkle_root.as_slice());
        stream.put_u32_le(self.timestamp);
        stream.put_u32_le(self.bits);
        stream.put_u32_le(self.nonce);
    }
}
