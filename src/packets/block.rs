use super::{
    EMPTY_HASH,
    blockheader::{BlockHeaderBorrowed, BlockHeaderOwned},
    buffer::Buffer,
    deserialize_array,
    packetpayload::{DeserializableBorrowed, PacketPayload, Serializable},
    tx::{TxBorrowed, TxOwned},
};
use crate::util::{arena::Arena, merkle::MerkleTree};
use anyhow::Result;
use anyhow::bail;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Default)]
pub struct BlockBorrowed<'a> {
    pub header: BlockHeaderBorrowed<'a>,
    pub txs: &'a [TxBorrowed<'a>],
}

pub const BLOCK_COMMAND: [u8; 12] = *b"block\0\0\0\0\0\0\0";

impl<'a> PacketPayload<'a, BlockOwned> for BlockBorrowed<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &BLOCK_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for BlockBorrowed<'a> {
    fn deserialize_borrowed(
        &mut self,
        allocator: &'a Arena,
        buffer: &'a [u8],
    ) -> anyhow::Result<usize> {
        self.header.deserialize_borrowed(allocator, buffer)?;
        // the header deserialization will read the number of txs, but the `txs` deserializer will also read it.
        // Start deserializing from offset 80 instead of the offset returned by header deserializer, because the header's size is 80+txlenlen

        let (txs, offset) = deserialize_array(allocator, buffer.with_offset(80)?)?;
        self.txs = txs;

        // Verify that this block is `correct`, as in the transactions inside actually correspond to the merkle tree in the header.
        // Compute the merkle root and make sure it's the same as in the header
        let mut txs_count = txs.len();
        if txs_count & 1 == 1 {
            txs_count += 1;
        }
        let mut merkle_tree = MerkleTree::with_capacity(txs_count);
        for tx in txs.iter() {
            merkle_tree.append_hash(&tx.hash);
        }
        let (merkle_root, mutated) = merkle_tree.into_root();
        if mutated {
            bail!("merkle root mutated")
        }
        if merkle_root != *self.header.merkle_root {
            bail!("merkle root does not match")
        }

        Ok(offset + 80)
    }
}

impl BlockBorrowed<'_> {
    pub fn verify_witness_commitment(&self) -> Result<()> {
        // verify that the witness data is present if the coinbase transaction indicates that this is a witness block
        let coinbase_tx = &self.txs[0];
        for txout in coinbase_tx.txouts.iter().rev() {
            if txout.script.len() >= 38
                && txout.script[0..6] == [0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed]
            {
                // This block has witness data
                if coinbase_tx.witness_data.is_none() {
                    bail!("missing witness data for coinbase tx in a witness block")
                }
                let coinbase_witness = coinbase_tx.witness_data.unwrap()[0];
                if coinbase_witness.len() != 34 || coinbase_witness[0..2] != [0x01, 32] {
                    bail!("invalid coinbase witness in a witness block")
                }

                let witness_commitment = txout.script.get_hash(6).unwrap();
                let witness_reserve_value = &coinbase_witness[2..34];

                let mut witness_tree = MerkleTree::with_capacity(self.txs.len());
                for i in 0..self.txs.len() {
                    if i == 0 {
                        witness_tree.append_hash(&EMPTY_HASH);
                    } else {
                        witness_tree.append_hash(&self.txs[i].witness_hash);
                    }
                }
                let (witness_root, mutated) = witness_tree.into_root();
                if mutated {
                    bail!("witness root mutated")
                }

                let mut combined_root_and_reserve_value = [0u8; 64];
                combined_root_and_reserve_value[0..32].copy_from_slice(&witness_root);
                combined_root_and_reserve_value[32..64].copy_from_slice(witness_reserve_value);
                let first_pass: [u8; 32] = Sha256::digest(combined_root_and_reserve_value).into();
                let second_pass: [u8; 32] = Sha256::digest(first_pass).into();

                if *witness_commitment != second_pass {
                    bail!(
                        "witness commitment is invalid, computed: {}, from txout: {}",
                        hex::encode(second_pass),
                        hex::encode(witness_commitment)
                    )
                }

                break;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct BlockOwned {
    pub header: BlockHeaderOwned,
    pub txs: Vec<TxOwned>,
}

impl Serializable for BlockOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        self.header.serialize(stream);
        // header.serialize will already add the number of txs, so we cant use serialize_array here
        for tx in &self.txs {
            tx.serialize(stream);
        }
    }
}

impl From<BlockBorrowed<'_>> for BlockOwned {
    fn from(value: BlockBorrowed<'_>) -> Self {
        Self {
            header: value.header.into(),
            txs: value.txs.iter().map(|v| (*v).into()).collect(),
        }
    }
}
