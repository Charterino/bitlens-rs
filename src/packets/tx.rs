use anyhow::{Result, anyhow, bail};
use bumpalo::{Bump, collections::Vec};
use sha2::{Digest, Sha256};

use super::{
    buffer::Buffer,
    packetpayload::{PacketPayload, Serializable, SerializableValue},
    varint::VarInt,
    varstr::VarStr,
};

pub struct Tx<'a> {
    pub version: u32,
    pub locktime: u32,
    pub txins: Option<&'a [&'a TxIn<'a>]>,
    pub txouts: Option<&'a [&'a TxOut<'a>]>,
    pub witness_data: Option<&'a [&'a [VarStr<'a>]]>,

    pub hash: [u8; 32],
}

pub const TX_COMMAND: [u8; 12] = *b"tx\0\0\0\0\0\0\0\0\0\0";

impl<'a> PacketPayload<'a> for Tx<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &TX_COMMAND
    }
}

impl<'a> Serializable<'a> for Tx<'a> {
    fn deserialize(allocator: &'a Bump<1>, buffer: &'a [u8]) -> Result<(&'a Tx<'a>, usize)> {
        let version = buffer.get_u32_le(0)?;

        let has_witness_data = match buffer.get(4..6) {
            Some(v) => *v == [0x00, 0x01],
            None => return Err(anyhow!("not enough data")),
        };

        let mut offset = if has_witness_data { 6 } else { 4 };

        let (txins, offset_delta) = <Option<&'a [&'a TxIn<'a>]> as SerializableValue>::deserialize(
            allocator,
            buffer.with_offset(offset)?,
        )?;
        offset += offset_delta;
        let txins_count = match &txins {
            Some(a) => a.len(),
            None => 0,
        };
        if txins_count == 0 {
            bail!("0 txins")
        }
        let (txouts, offset_delta) =
            <Option<_> as SerializableValue>::deserialize(allocator, buffer.with_offset(offset)?)?;
        offset += offset_delta;

        let (witnesses, witnesses_start) = if has_witness_data {
            let start = offset;
            let mut wits = Vec::with_capacity_in(txins_count, allocator);
            for i in 0..txins_count {
                let (components_count, offset_delta) =
                    VarInt::deserialize(allocator, buffer.with_offset(offset)?)?;
                offset += offset_delta;
                let mut components = Vec::with_capacity_in(components_count as usize, allocator);
                for j in 0..components_count as usize {
                    let (component, offset_delta) =
                        VarStr::deserialize(allocator, buffer.with_offset(offset)?)?;
                    offset += offset_delta;
                    components.push(component);
                }
                wits.push(components.into_bump_slice());
            }
            (Some(wits.into_bump_slice()), start)
        } else {
            (None, 0)
        };
        let witnesses_end = offset;

        let locktime = buffer.get_u32_le(offset)?;

        let first_pass = if has_witness_data {
            let mut sha = Sha256::new();
            sha.update(buffer.get(0..4).unwrap());
            sha.update(buffer.get(6..witnesses_start).unwrap());
            sha.update(buffer.get(witnesses_end..witnesses_end + 4).unwrap());
            sha.finalize()
        } else {
            Sha256::digest(buffer.get(0..offset + 4).unwrap())
        };
        let second_pass = Sha256::digest(first_pass);

        let result = allocator.alloc(Tx {
            version,
            locktime,
            txins,
            txouts,
            witness_data: witnesses,
            hash: second_pass.into(),
        });

        Ok((result, offset + 4))
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

pub struct TxIn<'a> {
    pub prevout_hash: &'a [u8; 32],
    pub prevout_index: u32,
    pub sequence: u32,
    pub sig_script: VarStr<'a>,
}

impl<'a> Serializable<'a> for TxIn<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a TxIn<'a>, usize)> {
        let hash = buffer.get_hash(0)?;
        let index = buffer.get_u32_le(32)?;
        let (script, offset) = VarStr::deserialize(allocator, buffer.with_offset(36)?)?;
        let sequence = buffer.get_u32_le(offset + 36)?;
        Ok((
            allocator.alloc(TxIn {
                prevout_hash: hash,
                prevout_index: index,
                sequence,
                sig_script: script,
            }),
            offset + 40,
        ))
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

impl<'a> Serializable<'a> for TxOut<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a TxOut<'a>, usize)> {
        let value = buffer.get_u64_le(0)?;
        let (script, offset) = VarStr::deserialize(allocator, buffer.with_offset(8)?)?;
        Ok((allocator.alloc(TxOut { value, script }), offset + 8))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64_le(self.value);
        self.script.serialize(stream);
    }
}
