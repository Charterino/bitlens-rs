use std::borrow::Cow;

use super::{
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable, SerializableValue},
    tx::Tx,
};

#[derive(Debug, Clone, Default)]
pub struct Block<'a> {
    pub version: u32,
    pub timestamp: u32,
    pub bits: u32,
    pub nonce: u32,
    pub parent: Cow<'a, [u8; 32]>,
    pub merkle_root: Cow<'a, [u8; 32]>,

    pub txs: Cow<'a, [Cow<'a, Tx<'a>>]>,
}

pub const BLOCK_COMMAND: [u8; 12] = *b"block\0\0\0\0\0\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for Block<'old> {
    fn command(&self) -> &'static [u8; 12] {
        &BLOCK_COMMAND
    }
}

impl<'old> MustOutlive<'old> for Block<'old> {
    type WithLifetime<'new: 'old> = Block<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for Block<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let parent = self.parent.clone().into_owned();
        let merkle_root = self.merkle_root.clone().into_owned();
        // once again suboptimal but it will do for now
        let txs = (&*self.txs)
            .deep_clone()
            .into_iter()
            .map(|x| Cow::Owned(x))
            .collect();
        Self::WithLifetime {
            version: self.version,
            timestamp: self.timestamp,
            bits: self.bits,
            nonce: self.nonce,
            parent: Cow::Owned(parent),
            merkle_root: Cow::Owned(merkle_root),
            txs: Cow::Owned(txs),
        }
    }
}

impl<'a> Serializable<'a> for Block<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a Block<'a>, usize)> {
        let version = buffer.get_u32_le(0)?;
        let parent = Cow::Borrowed(buffer.get_hash(4)?);
        let merkle_root = Cow::Borrowed(buffer.get_hash(36)?);
        let timestamp = buffer.get_u32_le(68)?;
        let bits = buffer.get_u32_le(72)?;
        let nonce = buffer.get_u32_le(76)?;

        let (txs, offset) = <Cow<'a, [Cow<'a, Tx<'a>>]> as SerializableValue>::deserialize(
            allocator,
            buffer.with_offset(80)?,
        )?;

        let result = allocator.alloc(Block {
            version,
            timestamp,
            bits,
            nonce,
            parent,
            merkle_root,
            txs: txs,
        });
        Ok((result, offset + 80))
    }

    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
