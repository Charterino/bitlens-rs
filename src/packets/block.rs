use super::{
    SupercowVec,
    blockheader::BlockHeader,
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable, SerializableSupercowVecOfCows},
    tx::Tx,
};
use crate::util::{arena::Arena, merkle::MerkleTree};
use anyhow::bail;
use supercow::Supercow;

#[derive(Debug, Clone)]
pub struct Block<'a> {
    pub header: Supercow<'a, BlockHeader<'a>>,
    pub txs: Supercow<'a, SupercowVec<'a, Tx<'a>>>,
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
        let txs = (&*self.txs.inner)
            .deep_clone()
            .into_iter()
            .map(Supercow::owned)
            .collect();
        Self::WithLifetime {
            header: Supercow::owned(header),
            txs: Supercow::owned(SupercowVec {
                inner: Supercow::owned(txs),
            }),
        }
    }
}

impl<'a> Serializable<'a> for Block<'a> {
    fn deserialize(
        allocator: &'a Arena,
        buffer: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, Block<'a>>, usize)> {
        let (header, _) = BlockHeader::deserialize(allocator, buffer)?;
        // the header deserialization will read the number of txs, but the `txs` deserializer will also read it.
        // Start deserializing from offset 80 instead of the offset returned by header deserializer, because the header's size is 80+txlenlen

        let (txs, offset) = SupercowVec::<Tx<'a>>::deserialize(allocator, buffer.with_offset(80)?)?;

        // Verify that this block is `correct`, as in the transactions inside actually correspond to the merkle tree in the header.
        // Compute the merkle root and make sure it's the same as in the header
        let mut txs_count = txs.inner.len();
        if txs_count & 1 == 1 {
            txs_count += 1;
        }
        let mut merkle_tree = MerkleTree::with_capacity(txs_count);
        for tx in txs.inner.iter() {
            merkle_tree.append_hash(&tx.hash);
        }
        let (merkle_root, mutated) = merkle_tree.into_root();
        if mutated {
            bail!("merkle root mutated")
        }
        if merkle_root != *header.merkle_root {
            bail!("merkle root does not match")
        }

        match allocator.try_alloc(Block {
            header,
            txs: Supercow::borrowed(txs),
        }) {
            Ok(result) => Ok((Supercow::borrowed(result), offset + 80)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        self.header.serialize(stream);
        for tx in self.txs.inner.iter() {
            tx.serialize(stream);
        }
    }
}
