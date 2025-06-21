use super::{
    SupercowVec,
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{
        PacketPayload, Serializable, SerializableSupercowVecOfCows, SerializableValue,
    },
    varint::VarInt,
    varstr::VarStr,
};
use anyhow::{Result, anyhow, bail};
use bumpalo::{Bump, collections::Vec};
use sha2::{Digest, Sha256};
use supercow::Supercow;

#[derive(Debug, Clone, Default)]
pub struct Tx<'a> {
    pub version: u32,
    pub locktime: u32,
    pub txins: SupercowVec<'a, TxIn<'a>>,
    pub txouts: SupercowVec<'a, TxOut<'a>>,
    pub witness_data: Option<SupercowVec<'a, SupercowVec<'a, VarStr<'a>>>>,
    pub hash: [u8; 32], // calculated during deserialization
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
        let txins = (&*self.txins.inner)
            .deep_clone()
            .into_iter()
            .map(Supercow::owned)
            .collect();
        let txouts = (&*self.txouts.inner)
            .deep_clone()
            .into_iter()
            .map(Supercow::owned)
            .collect();

        Self::WithLifetime {
            version: self.version,
            locktime: self.locktime,
            txins: SupercowVec {
                inner: Supercow::owned(txins),
            },
            txouts: SupercowVec {
                inner: Supercow::owned(txouts),
            },
            witness_data: None,
            hash: self.hash,
        }
    }
}

impl<'a> Serializable<'a> for Tx<'a> {
    fn deserialize(
        allocator: &'a Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(Supercow<'a, Tx<'a>>, usize)> {
        let version = buffer.get_u32_le(0)?;

        let has_witness_data = match buffer.get(4..6) {
            Some(v) => *v == [0x00, 0x01],
            None => return Err(anyhow!("not enough data")),
        };

        let mut offset = if has_witness_data { 6 } else { 4 };

        let (txins, offset_delta) =
            SupercowVec::deserialize(allocator, buffer.with_offset(offset)?)?;
        offset += offset_delta;
        if txins.inner.is_empty() {
            bail!("0 txins")
        }

        let (txouts, offset_delta) =
            SupercowVec::deserialize(allocator, buffer.with_offset(offset)?)?;
        offset += offset_delta;

        let (witnesses, witnesses_start) = if has_witness_data {
            let start = offset;
            let mut wits = Vec::with_capacity_in(txins.inner.len(), allocator);
            for _ in 0..txins.inner.len() {
                let (components, offset_delta) =
                    SupercowVec::deserialize(allocator, buffer.with_offset(offset)?)?;
                offset += offset_delta;
                wits.push(Supercow::owned(components));
            }
            let bump_slice = wits.into_bump_slice();
            let wits = SupercowVec {
                inner: Supercow::borrowed(bump_slice),
            };
            (Some(wits), start)
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

        match allocator.try_alloc(Tx {
            version,
            locktime,
            txins,
            txouts,
            witness_data: witnesses,
            hash: second_pass.into(),
        }) {
            Ok(result) => Ok((Supercow::borrowed(result), offset + 4)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.version);
        // If we have witness data, insert 0x0001 marker
        if self.witness_data.is_some() {
            stream.put_u16(0x0001);
        }
        let length = self.txins.inner.len() as VarInt;
        length.serialize(stream);
        for n in 0..length as usize {
            self.txins.inner[n].serialize(stream);
        }

        let length = self.txouts.inner.len() as VarInt;
        length.serialize(stream);
        for n in 0..length as usize {
            Serializable::serialize(&*self.txouts.inner[n], stream);
        }

        if let Some(witnesses) = &self.witness_data {
            for i in 0..witnesses.inner.len() {
                let witness = &witnesses.inner[i];
                let witness_length = witness.inner.len() as VarInt;
                witness_length.serialize(stream);
                for _ in 0..witness_length as usize {
                    let witness_component = &witness.inner[i];
                    Serializable::serialize(&**witness_component, stream);
                }
            }
        }

        stream.put_u32_le(self.locktime);
    }
}

#[derive(Debug, Clone)]
pub struct TxIn<'a> {
    pub prevout_hash: Supercow<'a, [u8; 32]>,
    pub prevout_index: u32,
    pub sequence: u32,
    pub sig_script: Supercow<'a, VarStr<'a>>,
}

impl<'old> MustOutlive<'old> for TxIn<'old> {
    type WithLifetime<'new: 'old> = TxIn<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for TxIn<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let prevout_hash = *self.prevout_hash;
        let sig_script = self.sig_script.deep_clone();
        Self::WithLifetime {
            prevout_hash: Supercow::owned(prevout_hash),
            prevout_index: self.prevout_index,
            sequence: self.sequence,
            sig_script: Supercow::owned(sig_script),
        }
    }
}

impl<'a> Serializable<'a> for TxIn<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, TxIn<'a>>, usize)> {
        let hash = buffer.get_hash(0)?;
        let index = buffer.get_u32_le(32)?;
        let (script, offset) =
            <VarStr as Serializable>::deserialize(allocator, buffer.with_offset(36)?)?;
        let sequence = buffer.get_u32_le(offset + 36)?;
        match allocator.try_alloc(TxIn {
            prevout_hash: Supercow::borrowed(hash),
            prevout_index: index,
            sequence,
            sig_script: script,
        }) {
            Ok(result) => Ok((Supercow::borrowed(result), offset + 40)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put(self.prevout_hash.as_slice());
        stream.put_u32_le(self.prevout_index);
        Serializable::serialize(&*self.sig_script, stream);
        stream.put_u32_le(self.sequence);
    }
}

#[derive(Clone, Debug)]
pub struct TxOut<'a> {
    pub value: u64,
    pub script: Supercow<'a, VarStr<'a>>,
}

impl<'old> MustOutlive<'old> for TxOut<'old> {
    type WithLifetime<'new: 'old> = TxOut<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for TxOut<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let script = self.script.deep_clone();
        Self::WithLifetime {
            value: self.value,
            script: Supercow::owned(script),
        }
    }
}

impl<'a> Serializable<'a> for TxOut<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, TxOut<'a>>, usize)> {
        let value = buffer.get_u64_le(0)?;
        let (script, offset) =
            <VarStr as Serializable>::deserialize(allocator, buffer.with_offset(8)?)?;
        match allocator.try_alloc(TxOut { value, script }) {
            Ok(result) => Ok((Supercow::borrowed(result), offset + 8)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64_le(self.value);
        Serializable::serialize(&*self.script, stream);
    }
}

impl<'script> SerializableValue<'script> for TxOut<'static> {
    fn deserialize(buffer: &'script [u8]) -> Result<(Self, usize)> {
        let value = buffer.get_u64_le(0)?;
        let (script, offset) = <VarStr as SerializableValue>::deserialize(buffer.with_offset(8)?)?;
        Ok((
            TxOut {
                value,
                script: Supercow::owned(script),
            },
            offset + 8,
        ))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64_le(self.value);
        Serializable::serialize(&*self.script, stream);
    }
}

impl<'short, 'long: 'short> TxOut<'long> {
    pub fn covariant(&self) -> &TxOut<'short> {
        unsafe { std::mem::transmute(self) }
    }
}
