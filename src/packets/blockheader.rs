use std::borrow::Cow;

use anyhow::bail;
use primitive_types::U256;
use sha2::{Digest, Sha256};

use crate::util::compact::u256_from_compact;

use super::{
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::Serializable,
    packetpayload::SerializableValue,
    varint::VarInt,
};

#[derive(Clone, Debug, Default)]
pub struct BlockHeader<'a> {
    pub version: u32,
    pub parent: Cow<'a, [u8; 32]>,
    pub merkle_root: Cow<'a, [u8; 32]>,
    pub timestamp: u32,
    pub bits: u32,
    pub nonce: u32,
    pub txs_count: VarInt, // Always present

    pub hash: [u8; 32], // calculated during deserialization
}

impl BlockHeader<'_> {
    pub fn construct(
        version: u32,
        parent: [u8; 32],
        merkle_root: [u8; 32],
        timestamp: u32,
        bits: u32,
        nonce: u32,
        txs_count: VarInt,
    ) -> Self {
        let mut s = Self {
            version,
            parent: Cow::Owned(parent),
            merkle_root: Cow::Owned(merkle_root),
            timestamp,
            bits,
            nonce,
            txs_count,
            hash: [0u8; 32],
        };
        let mut b = Vec::with_capacity(88);
        s.serialize(&mut b);
        let hash = Sha256::digest(b.get(0..80).unwrap());
        let hash = Sha256::digest(hash).into();
        s.hash = hash;
        s
    }

    pub fn human_hash(&self) -> String {
        let mut h = self.hash;
        h.reverse();
        hex::encode(h)
    }

    pub fn get_work(&self) -> U256 {
        let uncompacted = u256_from_compact(self.bits);
        let mut inverted = uncompacted;
        inverted.0[0] = !inverted.0[0];
        inverted.0[1] = !inverted.0[1];
        inverted.0[2] = !inverted.0[2];
        inverted.0[3] = !inverted.0[3];
        let result = inverted / (uncompacted + 1);
        result + 1
    }

    // Checks whether the hash has at least `bits` amount of work.
    pub fn is_valid(&self) -> bool {
        let target = u256_from_compact(self.bits);
        let hash_number = U256::from_little_endian(&self.hash);
        hash_number < target
    }
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
            txs_count: self.txs_count,
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

        let (txs_count, offset) = VarInt::deserialize(allocator, buffer.with_offset(80)?)?;

        let hash = Sha256::digest(buffer.get(0..80).unwrap());
        let hash = Sha256::digest(hash).into();

        match allocator.try_alloc(Self {
            version,
            parent,
            merkle_root,
            timestamp,
            bits,
            nonce,
            hash,
            txs_count,
        }) {
            Ok(result) => {
                if result.is_valid() {
                    Ok((result, 80 + offset))
                } else {
                    bail!("invalid header")
                }
            }
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.version);
        stream.put_slice(self.parent.as_slice());
        stream.put_slice(self.merkle_root.as_slice());
        stream.put_u32_le(self.timestamp);
        stream.put_u32_le(self.bits);
        stream.put_u32_le(self.nonce);
        self.txs_count.serialize(stream);
    }
}
