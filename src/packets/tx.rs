use anyhow::bail;
use bumpalo::boxed::Box;
use tokio::io::AsyncReadExt;

use super::{
    packetpayload::{PacketPayload, Serializable},
    varint::VarInt,
    varstr::VarStr,
    vec::Vec,
};

#[derive(Default)]
pub struct Tx<'a> {
    pub version: u32,
    pub locktime: u32,
    pub txins: Option<Box<'a, Vec<'a, TxIn<'a>>>>,
    pub txouts: Option<Box<'a, Vec<'a, TxOut<'a>>>>,
    pub witness_data: Option<Box<'a, Vec<'a, Vec<'a, VarStr<'a>>>>>,
}

impl<'a> Clone for Tx<'a> {
    fn clone(&self) -> Self {
        if self.txins.is_some() || self.txouts.is_some() || self.witness_data.is_some() {
            panic!("can't clone non-default tx objects")
        }
        Self {
            version: self.version.clone(),
            locktime: self.locktime.clone(),
            txins: None,
            txouts: None,
            witness_data: None,
        }
    }
}

pub const TX_COMMAND: [u8; 12] = *b"tx\0\0\0\0\0\0\0\0\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for Tx<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &TX_COMMAND
    }
}

impl<'a, 'b> Serializable<'a, 'b> for Tx<'a> {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'b>>,
    ) -> anyhow::Result<()> {
        self.version = stream.read_u32_le().await?;
        let mut txin_count = 0 as VarInt;
        txin_count.deserialize(allocator, stream).await?;

        let mut has_witness_data = false;
        if txin_count == 0 {
            let marker = stream.read_u8().await?;
            if marker != 1 {
                bail!("expected 0001 marker, got 00{:x}", marker)
            }

            // read the real txin count
            txin_count.deserialize(allocator, stream).await?;
            has_witness_data = true;
        }

        let mut txins = bumpalo::vec![in allocator; TxIn::default(); txin_count as usize];
        for i in 0..txin_count as usize {
            txins
                .get_mut(i)
                .unwrap()
                .deserialize(allocator, stream)
                .await?;
        }
        let txinsb = Vec::Bumpalod(txins);
        self.txins = Some(Box::new_in(txinsb, allocator));

        let mut txout_count = 0 as VarInt;
        txout_count.deserialize(allocator, stream).await?;

        let mut txouts = bumpalo::vec![in allocator; TxOut::default(); txout_count as usize];
        for i in 0..txout_count as usize {
            txouts
                .get_mut(i)
                .unwrap()
                .deserialize(allocator, stream)
                .await?;
        }

        self.txouts = Some(Box::new_in(Vec::Bumpalod(txouts), allocator));

        if has_witness_data {
            let mut witnesses: bumpalo::collections::Vec<'a, Vec<'a, Option<Vec<'a, u8>>>> = bumpalo::vec![in allocator; Vec::Bumpalod(bumpalo::vec![in allocator; VarStr::default(); 0]); txin_count as usize];
            for i in 0..txin_count as usize {
                let witness = witnesses.get_mut(i).unwrap();
                let mut witness_size = 0 as VarInt;
                witness_size.deserialize(allocator, stream).await?;
                witness.reserve(witness_size as usize);
                for j in 0..witness_size as usize {
                    let mut component = VarStr::default();
                    component.deserialize(allocator, stream).await?;
                    witness.push(component);
                }
            }

            self.witness_data = Some(Box::new_in(Vec::Bumpalod(witnesses), allocator));
        }

        self.locktime = stream.read_u32_le().await?;

        Ok(())
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.version);
        // If we have witness data, insert 0x0001 marker
        match self.witness_data {
            Some(_) => {
                stream.put_u16(0x0001);
            }
            None => {}
        }
        match &self.txins {
            Some(txins) => {
                let length = txins.len() as VarInt;
                length.serialize(stream);
                for n in 0..length as usize {
                    txins.get(n).unwrap().serialize(stream);
                }
            }
            None => {
                stream.put_u8(0);
            }
        }

        match &self.txouts {
            Some(txouts) => {
                let length = txouts.len() as VarInt;
                length.serialize(stream);
                for n in 0..length as usize {
                    txouts.get(n).unwrap().serialize(stream);
                }
            }
            None => {
                stream.put_u8(0);
            }
        }

        if let Some(witnesses) = &self.witness_data {
            for i in 0..witnesses.len() {
                let witness = witnesses.get(i).unwrap();
                let witness_length = witness.len() as VarInt;
                witness_length.serialize(stream);
                for j in 0..witness_length as usize {
                    let witness_component = witness.get(j).unwrap();
                    witness_component.serialize(stream);
                }
            }
        }

        stream.put_u32_le(self.locktime);
    }
}

#[derive(Default, Clone)]
pub struct TxIn<'a> {
    pub prevout_hash: [u8; 32],
    pub prevout_index: u32,
    pub sequence: u32,
    pub sig_script: VarStr<'a>,
}

impl<'a, 'b> Serializable<'a, 'b> for TxIn<'a> {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'b>>,
    ) -> anyhow::Result<()> {
        stream.read_exact(&mut self.prevout_hash).await?;
        self.prevout_index = stream.read_u32_le().await?;
        self.sig_script.deserialize(allocator, stream).await?;
        self.sequence = stream.read_u32_le().await?;
        Ok(())
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put(self.prevout_hash.as_slice());
        stream.put_u32_le(self.prevout_index);
        self.sig_script.serialize(stream);
        stream.put_u32_le(self.sequence);
    }
}

#[derive(Default, Clone)]
pub struct TxOut<'a> {
    pub value: u64,
    pub script: VarStr<'a>,
}

impl<'a, 'b> Serializable<'a, 'b> for TxOut<'a> {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'b>>,
    ) -> anyhow::Result<()> {
        self.value = stream.read_u64_le().await?;
        self.script.deserialize(allocator, stream).await?;
        Ok(())
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64_le(self.value);
        self.script.serialize(stream);
    }
}
