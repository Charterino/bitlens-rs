use super::{
    EMPTY_HASH,
    buffer::Buffer,
    packetpayload::{DeserializableBorrowed, Serializable},
    varint::{VarInt, deserialize_varint, serialize_varint},
};
use crate::util::{arena::Arena, compact::u256_from_compact};
use anyhow::bail;
use primitive_types::U256;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Copy)]
pub struct BlockHeaderBorrowed<'a> {
    pub version: u32,
    pub parent: &'a [u8; 32],
    pub merkle_root: &'a [u8; 32],
    pub timestamp: u32,
    pub bits: u32,
    pub nonce: u32,
    pub txs_count: VarInt, // Always present

    pub hash: [u8; 32], // calculated during deserialization
}

impl Default for BlockHeaderBorrowed<'_> {
    fn default() -> Self {
        Self {
            version: Default::default(),
            parent: &EMPTY_HASH,
            merkle_root: &EMPTY_HASH,
            timestamp: Default::default(),
            bits: Default::default(),
            nonce: Default::default(),
            txs_count: Default::default(),
            hash: Default::default(),
        }
    }
}

impl<'a> DeserializableBorrowed<'a> for BlockHeaderBorrowed<'a> {
    fn deserialize_borrowed(&mut self, _: &'a Arena, buffer: &'a [u8]) -> anyhow::Result<usize> {
        self.version = buffer.get_u32_le(0)?;
        self.parent = buffer.get_hash(4)?;
        self.merkle_root = buffer.get_hash(36)?;
        self.timestamp = buffer.get_u32_le(68)?;
        self.bits = buffer.get_u32_le(72)?;
        self.nonce = buffer.get_u32_le(76)?;

        let (txs_count, offset) = deserialize_varint(buffer.with_offset(80)?)?;
        self.txs_count = txs_count;

        let hash = Sha256::digest(buffer.get(0..80).unwrap());
        self.hash = Sha256::digest(hash).into();

        if !self.is_valid() {
            bail!("invalid header")
        }
        Ok(80 + offset)
    }
}

impl BlockHeaderBorrowed<'_> {
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

#[derive(Clone, Debug, Copy, Default)]
pub struct BlockHeaderOwned {
    pub version: u32,
    pub parent: [u8; 32],
    pub merkle_root: [u8; 32],
    pub timestamp: u32,
    pub bits: u32,
    pub nonce: u32,
    pub txs_count: VarInt, // Always present

    pub hash: [u8; 32], // calculated during deserialization
}

impl Serializable for BlockHeaderOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.version);
        stream.put_slice(self.parent.as_slice());
        stream.put_slice(self.merkle_root.as_slice());
        stream.put_u32_le(self.timestamp);
        stream.put_u32_le(self.bits);
        stream.put_u32_le(self.nonce);
        serialize_varint(self.txs_count, stream);
    }
}

impl BlockHeaderOwned {
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
            parent,
            merkle_root,
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

impl From<BlockHeaderBorrowed<'_>> for BlockHeaderOwned {
    fn from(value: BlockHeaderBorrowed<'_>) -> Self {
        Self {
            version: value.version,
            parent: *value.parent,
            merkle_root: *value.merkle_root,
            timestamp: value.timestamp,
            bits: value.bits,
            nonce: value.nonce,
            txs_count: value.txs_count,
            hash: value.hash,
        }
    }
}
