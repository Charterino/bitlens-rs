use std::borrow::Cow;

use anyhow::bail;

use crate::util::merkle::compute_merkle_root;

use super::{
    blockheader::BlockHeader,
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable, SerializableValue},
    tx::Tx,
};

#[derive(Debug, Clone, Default)]
pub struct Block<'a> {
    pub header: Cow<'a, BlockHeader<'a>>,
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
        let header = self.header.deep_clone();
        // once again suboptimal but it will do for now
        let txs = (&*self.txs)
            .deep_clone()
            .into_iter()
            .map(Cow::Owned)
            .collect();
        Self::WithLifetime {
            header: Cow::Owned(header),
            txs: Cow::Owned(txs),
        }
    }
}

impl<'a> Serializable<'a> for Block<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a Block<'a>, usize)> {
        let (header, _) = BlockHeader::deserialize(allocator, buffer)?;
        // the header deserialization will read the number of txs, but the `txs` deserializer will also read it.
        // Start deserializing from offset 80 instead of the offset returned by header deserializer, because the header's size is 80+txlenlen

        let (txs, offset) = <Cow<'a, [Cow<'a, Tx<'a>>]> as SerializableValue>::deserialize(
            allocator,
            buffer.with_offset(80)?,
        )?;

        // Verify that this block is `correct`, as in the transactions inside actually correspond to the merkle tree in the header.

        // Compute the merkle root and make sure it's the same as in the header
        let (merkle_root, mutated) = compute_merkle_root(txs.iter().map(|x| x.hash).collect());
        if mutated {
            bail!("merkle root mutated")
        }
        if merkle_root != *header.merkle_root {
            bail!("merkle root does not match")
        }

        match allocator.try_alloc(Block {
            header: Cow::Borrowed(header),
            txs,
        }) {
            Ok(result) => Ok((result, offset + 80)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        self.header.serialize(stream);
        for tx in self.txs.iter() {
            tx.serialize(stream);
        }
    }
}
