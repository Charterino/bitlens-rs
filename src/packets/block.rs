use std::borrow::Cow;

use super::{
    buffer::Buffer,
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

impl<'a> PacketPayload<'a> for Block<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &BLOCK_COMMAND
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
