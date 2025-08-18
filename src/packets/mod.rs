use crate::util::arena::Arena;
use anyhow::Result;
use buffer::Buffer;
use bytes::BufMut;
use packetpayload::{DeserializableBorrowed, DeserializableOwned, Serializable};
use varint::{VarInt, deserialize_varint, serialize_varint};

pub mod addr;
pub mod addrv2;
pub mod block;
pub mod blockheader;
pub mod buffer;
pub mod getaddr;
pub mod getdata;
pub mod getheaders;
pub mod headers;
pub mod inv;
pub mod invvector;
pub mod magic;
pub mod netaddr;
pub mod network_id;
pub mod packet;
#[cfg(test)]
pub mod packet_test;
pub mod packetheader;
pub mod packetpayload;
pub mod ping;
pub mod pong;
pub mod sendaddrv2;
pub mod sendheaders;
pub mod tx;
pub mod varint;
pub mod varstr;
pub mod verack;
pub mod version;

pub static EMPTY_HASH: [u8; 32] = [0u8; 32];

pub fn deserialize_array<'a, T: DeserializableBorrowed<'a> + Default + Copy>(
    allocator: &'a Arena,
    buffer: &'a [u8],
) -> Result<(&'a [T], usize)> {
    let (count, mut offset) = deserialize_varint(buffer)?;
    let array = allocator.try_alloc_array_fill_copy(count as usize, T::default())?;
    for item in array.iter_mut() {
        offset += item.deserialize_borrowed(allocator, buffer.with_offset(offset)?)?;
    }

    Ok((array, offset))
}

pub fn deserialize_array_owned<T: DeserializableOwned>(buffer: &[u8]) -> Result<(Vec<T>, usize)> {
    let (count, mut offset) = deserialize_varint(buffer)?;
    let mut vec = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (item, consumed) = T::deserialize_owned(buffer.with_offset(offset)?)?;
        offset += consumed;
        vec.push(item);
    }
    Ok((vec, offset))
}

pub fn serialize_array<T: Serializable>(array: &[T], into: &mut impl BufMut) {
    serialize_varint(array.len() as VarInt, into);
    for item in array {
        item.serialize(into);
    }
}
