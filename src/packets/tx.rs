use std::borrow::Cow;

use anyhow::{Result, anyhow, bail};
use bumpalo::{Bump, collections::Vec};
use sha2::{Digest, Sha256};

use super::{
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable, SerializableValue},
    varint::VarInt,
    varstr::VarStr,
};

#[derive(Debug, Clone, Default)]
pub struct Tx<'a> {
    pub version: u32,
    pub locktime: u32,
    pub txins: Cow<'a, [Cow<'a, TxIn<'a>>]>,
    pub txouts: Cow<'a, [Cow<'a, TxOut<'a>>]>,
    pub witness_data: Option<Cow<'a, [Cow<'a, [VarStr<'a>]>]>>,

    pub hash: [u8; 32],
}

pub const TX_COMMAND: [u8; 12] = *b"tx\0\0\0\0\0\0\0\0\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for Tx<'old> {
    fn command(&self) -> &'static [u8; 12] {
        &TX_COMMAND
    }
}

impl<'old> MustOutlive<'old> for Tx<'old> {
    type WithLifetime<'new: 'old> = Tx<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for Tx<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        // This is kind of suboptimal, but it'll do for now.
        let txins = (&*self.txins)
            .deep_clone()
            .into_iter()
            .map(Cow::Owned)
            .collect();
        let txouts = (&*self.txouts)
            .deep_clone()
            .into_iter()
            .map(Cow::Owned)
            .collect();

        Self::WithLifetime {
            version: self.version,
            locktime: self.locktime,
            txins: Cow::Owned(txins),
            txouts: Cow::Owned(txouts),
            witness_data: None,
            hash: self.hash,
        }
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

        let (txins, offset_delta) =
            <Cow<'a, [Cow<'a, TxIn<'a>>]> as SerializableValue>::deserialize(
                allocator,
                buffer.with_offset(offset)?,
            )?;
        offset += offset_delta;
        if txins.is_empty() {
            bail!("0 txins")
        }
        let (txouts, offset_delta) =
            <Cow<_> as SerializableValue>::deserialize(allocator, buffer.with_offset(offset)?)?;
        offset += offset_delta;

        let (witnesses, witnesses_start) = if has_witness_data {
            let start = offset;
            let mut wits = Vec::with_capacity_in(txins.len(), allocator);
            for _ in 0..txins.len() {
                let (components_count, offset_delta) =
                    VarInt::deserialize(allocator, buffer.with_offset(offset)?)?;
                offset += offset_delta;
                let mut components = Vec::with_capacity_in(components_count as usize, allocator);
                for _ in 0..components_count as usize {
                    let (component, offset_delta) =
                        VarStr::deserialize(allocator, buffer.with_offset(offset)?)?;
                    offset += offset_delta;
                    components.push(component);
                }
                wits.push(Cow::Borrowed(components.into_bump_slice()));
            }
            (Some(Cow::Borrowed(wits.into_bump_slice())), start)
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
        if self.witness_data.is_some() {
            stream.put_u16(0x0001);
        }
        let length = self.txins.len() as VarInt;
        length.serialize(stream);
        for n in 0..length as usize {
            self.txins.get(n).unwrap().serialize(stream);
        }

        let length = self.txouts.len() as VarInt;
        length.serialize(stream);
        for n in 0..length as usize {
            self.txouts.get(n).unwrap().serialize(stream);
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

#[derive(Default, Debug, Clone)]
pub struct TxIn<'a> {
    pub prevout_hash: Cow<'a, [u8; 32]>,
    pub prevout_index: u32,
    pub sequence: u32,
    pub sig_script: VarStr<'a>,
}

impl<'old> MustOutlive<'old> for TxIn<'old> {
    type WithLifetime<'new: 'old> = TxIn<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for TxIn<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let prevout_hash = self.prevout_hash.clone().into_owned();
        let sig_script = self.sig_script.deep_clone();
        Self::WithLifetime {
            prevout_hash: Cow::Owned(prevout_hash),
            prevout_index: self.prevout_index,
            sequence: self.sequence,
            sig_script,
        }
    }
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
                prevout_hash: Cow::Borrowed(hash),
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

#[derive(Default, Clone, Debug)]
pub struct TxOut<'a> {
    pub value: u64,
    pub script: VarStr<'a>,
}

impl<'old> MustOutlive<'old> for TxOut<'old> {
    type WithLifetime<'new: 'old> = TxOut<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for TxOut<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let script = self.script.deep_clone();
        Self::WithLifetime {
            value: self.value,
            script,
        }
    }
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
