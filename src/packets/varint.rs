use super::{buffer::Buffer, packetpayload::SerializableValue};
use anyhow::Result;
use bytes::BufMut;

pub type VarInt = u64;

impl<'a> SerializableValue<'a> for VarInt {
    fn deserialize(buffer: &'a [u8]) -> Result<(VarInt, usize)> {
        let leader = buffer[0];
        Ok(if leader < 0xFD {
            (leader as u64, 1)
        } else if leader == 0xFD {
            (buffer.get_u16_le(1)? as u64, 3)
        } else if leader == 0xFE {
            (buffer.get_u32_le(1)? as u64, 5)
        } else {
            (buffer.get_u64_le(1)?, 9)
        })
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        if *self < 0xFD {
            stream.put_u8(*self as u8);
        } else if *self < 0xFFFF {
            stream.put_u8(0xFD);
            stream.put_u16_le(*self as u16);
        } else if *self < 0xFFFFFFFF {
            stream.put_u8(0xFE);
            stream.put_u32_le(*self as u32);
        } else {
            stream.put_u8(0xFF);
            stream.put_u64_le(*self);
        }
    }
}
