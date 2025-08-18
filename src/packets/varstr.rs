use super::{
    buffer::Buffer,
    packetpayload::{DeserializableBorrowed, DeserializableOwned, Serializable},
    varint::{deserialize_varint, length_varint, serialize_varint},
};
use anyhow::{Result, bail};
use bytes::BufMut;

#[allow(dead_code)]
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

pub fn deserialize_array_of_varstr_as_varstr(buffer: &[u8]) -> Result<(&[u8], usize)> {
    let (remaining_varstrs, mut offset) = deserialize_varint(buffer)?;
    for _ in 0..remaining_varstrs {
        let (varstr_length, consumed) = deserialize_varint(buffer.with_offset(offset)?)?;
        offset += consumed;
        offset += varstr_length as usize;
    }

    match buffer.get(0..offset) {
        Some(r) => Ok((r, offset)),
        None => bail!("not enough bytes"),
    }
}

pub fn deserialize_array_of_varstr_as_varstr_owned(buffer: &[u8]) -> Result<(Vec<u8>, usize)> {
    let (remaining_varstrs, mut offset) = deserialize_varint(buffer)?;
    for _ in 0..remaining_varstrs {
        let (varstr_length, consumed) = deserialize_varint(buffer.with_offset(offset)?)?;
        offset += consumed;
        offset += varstr_length as usize;
    }

    match buffer.get(0..offset) {
        Some(r) => Ok((r.to_vec(), offset)),
        None => bail!("not enough bytes"),
    }
}

pub fn deserialize_array_of_varsrs_iter(buffer: &[u8]) -> Result<VarStrIter> {
    let (count, consumed) = deserialize_varint(buffer)?;
    Ok(VarStrIter {
        offset: consumed,
        buffer,
        remaining: count,
    })
}
pub struct VarStrIter<'a> {
    offset: usize,
    buffer: &'a [u8],
    remaining: u64,
}

impl<'a> Iterator for VarStrIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        match self.buffer.with_offset(self.offset) {
            Ok(v) => match deserialize_varstr(v) {
                Ok((result, consumed)) => {
                    self.offset += consumed;
                    self.remaining -= 1;
                    Some(result)
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    }
}
