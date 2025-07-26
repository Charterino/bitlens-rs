use crate::{packets::varstr::deserialize_array_of_varstr_as_varstr, util::arena::Arena};

use super::{
    EMPTY_HASH,
    buffer::Buffer,
    deserialize_array, deserialize_array_owned,
    packetpayload::{DeserializableBorrowed, DeserializableOwned, PacketPayload, Serializable},
    serialize_array,
    varint::{VarInt, length_varint, serialize_varint, serialize_varint_into_slice},
    varstr::{deserialize_array_of_varstr_as_varstr_owned, deserialize_varstr, serialize_varstr},
};
use anyhow::{Result, anyhow, bail};
use either::Either;
use sha2::{Digest, Sha256};
use slog_scope::info;

#[derive(Debug, Clone, Copy, Default)]
pub struct TxBorrowed<'a> {
    pub version: u32,
    pub locktime: u32,
    pub txins: &'a [TxInBorrowed<'a>],
    pub txouts: &'a [TxOutBorrowed<'a>],
    pub witness_data: Option<&'a [&'a [u8]]>,
    pub hash: [u8; 32], // calculated during deserialization
}

impl TxBorrowed<'_> {}

pub const TX_COMMAND: [u8; 12] = *b"tx\0\0\0\0\0\0\0\0\0\0";

impl<'a> PacketPayload<'a, TxOwned> for TxBorrowed<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &TX_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for TxBorrowed<'a> {
    fn deserialize_borrowed(&mut self, allocator: &'a Arena, buffer: &'a [u8]) -> Result<usize> {
        self.version = buffer.get_u32_le(0)?;

        let has_witness_data = match buffer.get(4..6) {
            Some(v) => *v == [0x00, 0x01],
            None => return Err(anyhow!("not enough data")),
        };

        let mut offset = if has_witness_data { 6 } else { 4 };

        let (txins, offset_delta) = deserialize_array(allocator, buffer.with_offset(offset)?)?;
        offset += offset_delta;
        if txins.is_empty() {
            bail!("0 txins")
        }
        self.txins = txins;

        let (txouts, offset_delta) = deserialize_array(allocator, buffer.with_offset(offset)?)?;
        offset += offset_delta;
        self.txouts = txouts;

        let (witnesses, witnesses_start) = if has_witness_data {
            let start = offset;
            let mut wits = allocator.try_alloc_arenaarray(txins.len())?;
            for _ in 0..txins.len() {
                let (components, offset_delta) =
                    //deserialize_array(allocator, buffer.with_offset(offset)?)?;
                    deserialize_array_of_varstr_as_varstr(buffer.with_offset(offset)?)?;
                offset += offset_delta;
                wits.push(components);
            }
            (Some(wits.into_arena_array()), start)
        } else {
            (None, 0)
        };
        self.witness_data = witnesses;
        let witnesses_end = offset;

        self.locktime = buffer.get_u32_le(offset)?;

        let first_pass = if has_witness_data {
            let mut sha = Sha256::new();
            sha.update(buffer.get(0..4).unwrap());
            sha.update(buffer.get(6..witnesses_start).unwrap());
            sha.update(buffer.get(witnesses_end..witnesses_end + 4).unwrap());
            sha.finalize()
        } else {
            Sha256::digest(buffer.get(0..offset + 4).unwrap())
        };
        self.hash = Sha256::digest(first_pass).into();

        Ok(offset + 4)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TxOwned {
    pub version: u32,
    pub locktime: u32,
    pub txins: Vec<TxInOwned>,
    pub txouts: Vec<TxOutOwned>,
    pub witness_data: Option<Vec<Vec<u8>>>,
    pub hash: [u8; 32], // calculated during deserialization
}

impl Serializable for TxOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.version);
        // If we have witness data, insert 0x0001 marker
        if self.witness_data.is_some() {
            stream.put_u16(0x0001);
        }

        serialize_array(&self.txins, stream);
        serialize_array(&self.txouts, stream);

        if let Some(witnesses) = &self.witness_data {
            serialize_array(witnesses, stream);
        }

        stream.put_u32_le(self.locktime);
    }
}

impl From<TxBorrowed<'_>> for TxOwned {
    fn from(value: TxBorrowed<'_>) -> Self {
        Self {
            version: value.version,
            locktime: value.locktime,
            txins: value.txins.iter().map(|v| (*v).into()).collect(),
            txouts: value.txouts.iter().map(|v| (*v).into()).collect(),
            witness_data: value
                .witness_data
                .map(|value| value.iter().map(|v| v.to_vec()).collect()),
            hash: value.hash,
        }
    }
}

impl TxOwned {
    pub fn deserialize_without_txouts(buffer: &[u8], hash: [u8; 32]) -> Result<TxOwned> {
        let version = buffer.get_u32_le(0)?;

        let has_witness_data = match buffer.get(4..6) {
            Some(v) => *v == [0x00, 0x01],
            None => return Err(anyhow!("not enough data")),
        };

        let mut offset = if has_witness_data { 6 } else { 4 };

        let (txins, offset_delta) = deserialize_array_owned(buffer.with_offset(offset)?)?;
        offset += offset_delta;
        if txins.is_empty() {
            bail!("0 txins")
        }

        let (witnesses, _) = if has_witness_data {
            let start = offset;
            let mut wits = Vec::with_capacity(txins.len());
            for _ in 0..txins.len() {
                let (components, offset_delta) =
                    //deserialize_array_owned(buffer.with_offset(offset)?)?;
                    deserialize_array_of_varstr_as_varstr_owned(buffer.with_offset(offset)?)?;
                offset += offset_delta;
                wits.push(components);
            }
            (Some(wits), start)
        } else {
            (None, 0)
        };

        let locktime = buffer.get_u32_le(offset)?;

        Ok(TxOwned {
            version,
            locktime,
            txins,
            txouts: vec![],
            witness_data: witnesses,
            hash,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TxRef<'a> {
    Borrowed(&'a TxBorrowed<'a>),
    Owned(&'a TxOwned),
}

impl TxRef<'_> {
    pub fn is_coinbase(&self) -> bool {
        let mut txins = self.txins();
        let first = txins.next();
        let second = txins.next();
        if second.is_some() {
            return false;
        }
        if let Some(txin) = first {
            txin.is_empty()
        } else {
            false
        }
    }

    pub fn txins(&self) -> impl Iterator<Item = TxInRef> {
        match self {
            TxRef::Borrowed(tx_borrowed) => {
                either::Left(tx_borrowed.txins.iter().map(TxInRef::Borrowed))
            }
            TxRef::Owned(tx_owned) => either::Right(tx_owned.txins.iter().map(TxInRef::Owned)),
        }
    }

    pub fn txouts(&self) -> impl Iterator<Item = TxOutRef> {
        match self {
            TxRef::Borrowed(tx_borrowed) => {
                either::Left(tx_borrowed.txouts.iter().map(TxOutRef::Borrowed))
            }
            TxRef::Owned(tx_owned) => either::Right(tx_owned.txouts.iter().map(TxOutRef::Owned)),
        }
    }

    pub fn witness_data(&self) -> Option<Either<&[Vec<u8>], &[&[u8]]>> {
        match self {
            TxRef::Borrowed(tx_borrowed) => tx_borrowed.witness_data.map(either::Right),
            TxRef::Owned(tx_owned) => tx_owned
                .witness_data
                .as_ref()
                .map(|wd| either::Left(wd.as_slice())),
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        match self {
            TxRef::Borrowed(tx_borrowed) => tx_borrowed.hash,
            TxRef::Owned(tx_owned) => tx_owned.hash,
        }
    }

    pub fn version(&self) -> u32 {
        match self {
            TxRef::Borrowed(tx_borrowed) => tx_borrowed.version,
            TxRef::Owned(tx_owned) => tx_owned.version,
        }
    }

    pub fn locktime(&self) -> u32 {
        match self {
            TxRef::Borrowed(tx_borrowed) => tx_borrowed.locktime,
            TxRef::Owned(tx_owned) => tx_owned.locktime,
        }
    }

    pub fn serialized_without_txouts_size(&self) -> usize {
        let mut total = 4; // version
        if self.witness_data().is_some() {
            total += 2; // witness marker
        }
        total += length_varint(self.txins().count() as VarInt); // txins len len
        for txin in self.txins() {
            total += 40; // hash + index + sequence
            total += length_varint(txin.sig_script().len() as VarInt); // script len len
            total += txin.sig_script().len(); // script len
        }

        if let Some(witnesses) = &self.witness_data() {
            match witnesses {
                Either::Left(witnesses) => {
                    for i in 0..witnesses.len() {
                        let witness = &witnesses[i];
                        total += length_varint(witness.len() as VarInt); // component count len
                        total += witness.len();
                        /*for j in 0..witness.len() {
                            let component = &witness[j];
                            total += length_varint(component.len() as VarInt); // component size len
                            total += component.len(); // component len
                        }*/
                    }
                }
                Either::Right(witnesses) => {
                    for i in 0..witnesses.len() {
                        let witness = witnesses[i];
                        total += length_varint(witness.len() as VarInt); // component count len
                        total += witness.len();
                        /*for j in 0..witness.len() {
                            let component = witness[j];
                            total += length_varint(component.len() as VarInt); // component size len
                            total += component.len(); // component len
                        }*/
                    }
                }
            }
        }

        total += 4; // locktime

        total
    }

    pub fn serialize_without_txouts(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.version());
        // If we have witness data, insert 0x0001 marker
        if self.witness_data().is_some() {
            stream.put_u16(0x0001);
        }
        serialize_varint(self.txins().count() as VarInt, stream);
        for txin in self.txins() {
            txin.serialize(stream);
        }

        if let Some(witnesses) = self.witness_data() {
            match witnesses {
                Either::Left(witnesses) => {
                    for witness in witnesses.iter() {
                        /*serialize_varint(witness.len() as VarInt, stream);
                        for witness_component in witness.iter() {
                            serialize_varstr(witness_component, stream);
                        }*/
                        serialize_varstr(&witness, stream);
                    }
                }
                Either::Right(witnesses) => {
                    for witness in witnesses.iter() {
                        /*serialize_varint(witness.len() as VarInt, stream);
                        for witness_component in witness.iter() {
                            serialize_varstr(witness_component, stream);
                        }*/
                        serialize_varstr(witness, stream);
                    }
                }
            }
        }

        stream.put_u32_le(self.locktime());
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TxInBorrowed<'a> {
    pub prevout_hash: &'a [u8; 32],
    pub prevout_index: u32,
    pub sequence: u32,
    pub sig_script: &'a [u8],
}

impl Default for TxInBorrowed<'_> {
    fn default() -> Self {
        Self {
            prevout_hash: &EMPTY_HASH,
            prevout_index: Default::default(),
            sequence: Default::default(),
            sig_script: Default::default(),
        }
    }
}

impl<'a> DeserializableBorrowed<'a> for TxInBorrowed<'a> {
    fn deserialize_borrowed(&mut self, _: &'a Arena, buffer: &'a [u8]) -> anyhow::Result<usize> {
        self.prevout_hash = buffer.get_hash(0)?;
        self.prevout_index = buffer.get_u32_le(32)?;
        let (script, offset) = deserialize_varstr(buffer.with_offset(36)?)?;
        self.sig_script = script;
        self.sequence = buffer.get_u32_le(offset + 36)?;
        Ok(offset + 40)
    }
}

impl Serializable for TxInBorrowed<'_> {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put(self.prevout_hash.as_slice());
        stream.put_u32_le(self.prevout_index);
        serialize_varstr(self.sig_script, stream);
        stream.put_u32_le(self.sequence);
    }
}

#[derive(Debug, Clone, Default)]
pub struct TxInOwned {
    pub prevout_hash: [u8; 32],
    pub prevout_index: u32,
    pub sequence: u32,
    pub sig_script: Vec<u8>,
}

impl DeserializableOwned for TxInOwned {
    fn deserialize_owned(buffer: &[u8]) -> Result<(Self, usize)> {
        let hash = buffer.get_hash(0)?;
        let index = buffer.get_u32_le(32)?;
        let (script, offset) = deserialize_varstr(buffer.with_offset(36)?)?;
        let sequence = buffer.get_u32_le(offset + 36)?;
        Ok((
            TxInOwned {
                prevout_hash: *hash,
                prevout_index: index,
                sequence,
                sig_script: script.to_vec(),
            },
            offset + 40,
        ))
    }
}

impl Serializable for TxInOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put(self.prevout_hash.as_slice());
        stream.put_u32_le(self.prevout_index);
        serialize_varstr(&self.sig_script, stream);
        stream.put_u32_le(self.sequence);
    }
}

impl From<TxInBorrowed<'_>> for TxInOwned {
    fn from(value: TxInBorrowed<'_>) -> Self {
        Self {
            prevout_hash: *value.prevout_hash,
            prevout_index: value.prevout_index,
            sequence: value.sequence,
            sig_script: value.sig_script.to_vec(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TxInRef<'a> {
    Borrowed(&'a TxInBorrowed<'a>),
    Owned(&'a TxInOwned),
}

impl TxInRef<'_> {
    pub fn sig_script(&self) -> &[u8] {
        match self {
            TxInRef::Borrowed(tx_in_borrowed) => tx_in_borrowed.sig_script,
            TxInRef::Owned(tx_in_owned) => &tx_in_owned.sig_script,
        }
    }

    pub fn prevout_hash(&self) -> [u8; 32] {
        match self {
            TxInRef::Borrowed(tx_in_borrowed) => *tx_in_borrowed.prevout_hash,
            TxInRef::Owned(tx_in_owned) => tx_in_owned.prevout_hash,
        }
    }

    pub fn prevout_index(&self) -> u32 {
        match self {
            TxInRef::Borrowed(tx_in_borrowed) => tx_in_borrowed.prevout_index,
            TxInRef::Owned(tx_in_owned) => tx_in_owned.prevout_index,
        }
    }

    pub fn sequence(&self) -> u32 {
        match self {
            TxInRef::Borrowed(tx_in_borrowed) => tx_in_borrowed.sequence,
            TxInRef::Owned(tx_in_owned) => tx_in_owned.sequence,
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put(self.prevout_hash().as_slice());
        stream.put_u32_le(self.prevout_index());
        serialize_varstr(self.sig_script(), stream);
        stream.put_u32_le(self.sequence());
    }
}

impl TxInRef<'_> {
    pub fn is_empty(&self) -> bool {
        match self {
            TxInRef::Borrowed(tx_in_borrowed) => {
                *tx_in_borrowed.prevout_hash == EMPTY_HASH
                    && tx_in_borrowed.prevout_index == 0xFFFFFFFF
            }
            TxInRef::Owned(tx_in_owned) => {
                tx_in_owned.prevout_hash == EMPTY_HASH && tx_in_owned.prevout_index == 0xFFFFFFFF
            }
        }
    }
}

#[derive(Clone, Debug, Copy, Default)]
pub struct TxOutBorrowed<'a> {
    pub value: u64,
    pub script: &'a [u8],
}

impl<'a> DeserializableBorrowed<'a> for TxOutBorrowed<'a> {
    fn deserialize_borrowed(&mut self, _: &'a Arena, buffer: &'a [u8]) -> anyhow::Result<usize> {
        self.value = buffer.get_u64_le(0)?;
        let (script, offset) = deserialize_varstr(buffer.with_offset(8)?)?;
        self.script = script;
        Ok(offset + 8)
    }
}

#[derive(Clone, Debug, Default)]
pub struct TxOutOwned {
    pub value: u64,
    pub script: Vec<u8>,
}

impl DeserializableOwned for TxOutOwned {
    fn deserialize_owned(buffer: &[u8]) -> anyhow::Result<(Self, usize)> {
        let value = buffer.get_u64_le(0)?;
        let (script, offset) = deserialize_varstr(buffer.with_offset(8)?)?;
        Ok((
            Self {
                value,
                script: script.to_vec(),
            },
            offset + 8,
        ))
    }
}

impl Serializable for TxOutOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64_le(self.value);
        serialize_varstr(&self.script, stream);
    }
}

impl From<TxOutBorrowed<'_>> for TxOutOwned {
    fn from(value: TxOutBorrowed<'_>) -> Self {
        Self {
            value: value.value,
            script: value.script.to_vec(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TxOutRef<'a> {
    Borrowed(&'a TxOutBorrowed<'a>),
    Owned(&'a TxOutOwned),
}

impl TxOutRef<'_> {
    pub fn value(&self) -> u64 {
        match self {
            TxOutRef::Borrowed(tx_out_borrowed) => tx_out_borrowed.value,
            TxOutRef::Owned(tx_out_owned) => tx_out_owned.value,
        }
    }

    pub fn script(&self) -> &[u8] {
        match self {
            TxOutRef::Borrowed(tx_out_borrowed) => tx_out_borrowed.script,
            TxOutRef::Owned(tx_out_owned) => &tx_out_owned.script,
        }
    }

    pub fn serialized_length(&self) -> usize {
        let s = self.script();
        8 + length_varint(s.len() as VarInt) + s.len()
    }

    pub fn flat_serialize(&self, slice: &mut [u8]) -> usize {
        let b = self.value().to_le_bytes();
        slice.get_mut(0..8).unwrap().copy_from_slice(b.as_slice());
        let script = self.script();
        let start_of_script =
            8 + serialize_varint_into_slice(script.len() as VarInt, &mut slice[8..]);
        slice[start_of_script..start_of_script + script.len()].copy_from_slice(script);
        start_of_script + script.len()
    }
}
