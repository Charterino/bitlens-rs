use anyhow::Result;
use bumpalo::{Bump, collections::Vec};
use bytes::BufMut;

use super::{
    block::Block, buffer::Buffer, ping::Ping, pong::Pong, sendaddrv2::SendAddrV2,
    sendheaders::SendHeaders, tx::Tx, varint::VarInt, verack::VerAck, version::Version,
};

pub trait PacketPayload<'bump>: Serializable<'bump> {
    fn command(&self) -> &'static [u8; 12];
}

pub trait Serializable<'bump>
where
    Self: Sized,
{
    // While the structs themselves are going to be allocated in the bump allocator,
    // the scripts and other byte-arrays are going to be referenced and not copied.
    fn deserialize(
        allocator: &'bump bumpalo::Bump<1>,
        buffer: &'bump [u8],
    ) -> Result<(&'bump Self, usize)>; // returned value is the length consumed

    fn serialize(&self, stream: &mut impl BufMut);
}

pub trait SerializableValue<'bump>
where
    Self: Sized,
{
    // While the structs themselves are going to be allocated in the bump allocator,
    // the scripts and other byte-arrays are going to be referenced and not copied.
    fn deserialize(
        allocator: &'bump bumpalo::Bump<1>,
        buffer: &'bump [u8],
    ) -> Result<(Self, usize)>; // returned value is the length consumed

    fn serialize(&self, stream: &mut impl BufMut);
}

pub enum PacketPayloadType<'a> {
    Version(&'a Version<'a>),
    //Addr(&'a Addr<'a>),
    //AddrV2(Box<'a, AddrV2<'a>>),
    //GetAddr(Box<'a, GetAddr>),
    Ping(&'a Ping),
    Pong(&'a Pong),
    VerAck(&'a VerAck),
    SendHeaders(&'a SendHeaders),
    SendAddrV2(&'a SendAddrV2),
    Tx(&'a Tx<'a>),
    Block(&'a Block<'a>),
}

impl<'a, T: Serializable<'a>> SerializableValue<'a> for Option<&'a [&'a T]> {
    fn deserialize(
        allocator: &'a Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(Option<&'a [&'a T]>, usize)> {
        let (len, mut offset) = VarInt::deserialize(allocator, buffer)?;
        if len == 0 {
            return Ok((None, 1));
        }

        let mut result: Vec<'a, &'a T> = Vec::with_capacity_in(len as usize, allocator);
        for i in 0..len as usize {
            let (value, offset_delta) = T::deserialize(allocator, buffer.with_offset(offset)?)?;
            offset += offset_delta;
            result.push(value);
        }

        Ok((Some(result.into_bump_slice()), offset))
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        match self {
            Some(array) => {
                let len = array.len() as VarInt;
                len.serialize(stream);
                for i in 0..len as usize {
                    array.get(i).unwrap().serialize(stream);
                }
            }
            None => stream.put_u8(0),
        }
    }
}

impl<'a, T: Serializable<'a>> Serializable<'a> for Option<&'a [&'a T]> {
    fn deserialize(
        allocator: &'a Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(&'a Option<&'a [&'a T]>, usize)> {
        let (len, mut offset) = VarInt::deserialize(allocator, buffer)?;
        if len == 0 {
            return Ok((allocator.alloc(None), 1));
        }

        let mut result: Vec<'a, &'a T> = Vec::with_capacity_in(len as usize, allocator);
        for i in 0..len as usize {
            let (value, offset_delta) = T::deserialize(allocator, buffer.with_offset(offset)?)?;
            offset += offset_delta;
            result.push(value);
        }

        let res = allocator.alloc(Some(result.into_bump_slice()));
        Ok((res, offset))
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        match self {
            Some(array) => {
                let len = array.len() as VarInt;
                len.serialize(stream);
                for i in 0..len as usize {
                    array.get(i).unwrap().serialize(stream);
                }
            }
            None => stream.put_u8(0),
        }
    }
}
