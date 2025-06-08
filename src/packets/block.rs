use tokio::io::AsyncReadExt;

use super::{
    packetpayload::{PacketPayload, Serializable},
    tx::Tx,
    varint::VarInt,
    vec::Vec,
};

#[derive(Default)]
pub struct Block<'a> {
    pub version: u32,
    pub timestamp: u32,
    pub bits: u32,
    pub nonce: u32,
    pub parent: [u8; 32],
    pub merkle_root: [u8; 32],

    pub txs: Option<Vec<'a, Tx<'a>>>,
}

pub const BLOCK_COMMAND: [u8; 12] = *b"block\0\0\0\0\0\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for Block<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &BLOCK_COMMAND
    }
}

impl<'a, 'b> Serializable<'a, 'b> for Block<'a> {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'b>>,
    ) -> anyhow::Result<()> {
        self.version = stream.read_u32_le().await?;
        stream.read_exact(&mut self.parent).await?;
        stream.read_exact(&mut self.merkle_root).await?;
        self.timestamp = stream.read_u32_le().await?;
        self.bits = stream.read_u32_le().await?;
        self.nonce = stream.read_u32_le().await?;
        let mut txs_count = 0 as VarInt;
        txs_count.deserialize(allocator, stream).await?;

        if txs_count == 0 {
            self.txs = None;
            return Ok(());
        }

        let mut txs = bumpalo::vec![in allocator; Tx::default(); txs_count as usize];
        for i in 0..txs_count as usize {
            txs.get_mut(i)
                .unwrap()
                .deserialize(allocator, stream)
                .await?;
        }

        self.txs = Some(Vec::Bumpalod(txs));

        Ok(())
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {}
}
