use anyhow::{Result, bail};
use bytes::BufMut;

use super::{
    packetpayload::{DeserializableBorrowed, DeserializableOwned, Serializable},
    varint::{deserialize_varint, length_varint, serialize_varint},
};

const fn length_varstr(v: &[u8]) -> usize {
    length_varint(v.len() as u64) + v.len()
}

pub fn serialize_varstr(v: &[u8], into: &mut impl BufMut) -> usize {
    let starting = serialize_varint(v.len() as u64, into);
    into.put_slice(v);
    starting + v.len()
}

pub fn deserialize_varstr(buffer: &[u8]) -> Result<(&[u8], usize)> {
    let (length, offset) = deserialize_varint(buffer)?;
    if buffer.len() < length as usize + offset {
        bail!("not enough data")
    }
    Ok((
        &buffer[offset..(offset + length as usize)],
        offset + length as usize,
    ))
}

impl<'a> DeserializableBorrowed<'a> for &'a [u8] {
    fn deserialize_borrowed(
        &mut self,
        _: &'a crate::util::arena::Arena,
        buffer: &'a [u8],
    ) -> Result<usize> {
        let (res, consumed) = deserialize_varstr(buffer)?;
        *self = res;
        Ok(consumed)
    }
}

impl Serializable for Vec<u8> {
    fn serialize(&self, stream: &mut impl BufMut) {
        serialize_varstr(self, stream);
    }
}

impl DeserializableOwned for Vec<u8> {
    fn deserialize_owned(buffer: &[u8]) -> Result<(Self, usize)> {
        let (res, consumed) = deserialize_varstr(buffer)?;
        Ok((res.to_vec(), consumed))
    }
}
