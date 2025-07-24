use super::buffer::Buffer;
use anyhow::{Result, bail};
use bytes::BufMut;

pub type VarInt = u64;

pub const fn length_varint(v: VarInt) -> usize {
    if v < 0xFD {
        1
    } else if v < 0xFFFF {
        3
    } else if v < 0xFFFFFFFF {
        5
    } else {
        9
    }
}

pub fn serialize_varint(v: VarInt, into: &mut impl BufMut) -> usize {
    if v < 0xFD {
        into.put_u8(v as u8);
        1
    } else if v < 0xFFFF {
        into.put_u8(0xFD);
        into.put_u16_le(v as u16);
        3
    } else if v < 0xFFFFFFFF {
        into.put_u8(0xFE);
        into.put_u32_le(v as u32);
        5
    } else {
        into.put_u8(0xFF);
        into.put_u64_le(v);
        9
    }
}

pub fn deserialize_varint(buffer: &[u8]) -> Result<(VarInt, usize)> {
    if buffer.is_empty() {
        bail!("not enough bytes for varint")
    }
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

pub fn serialize_varint_into_slice(v: VarInt, into: &mut [u8]) -> usize {
    if v < 0xFD {
        into[0] = v as u8;
        1
    } else if v < 0xFFFF {
        into[0] = 0xFD;
        let s = (v as u16).to_le_bytes();
        into[1..3].copy_from_slice(s.as_slice());
        3
    } else if v < 0xFFFFFFFF {
        into[0] = 0xFE;
        let s = (v as u32).to_le_bytes();
        into[1..5].copy_from_slice(s.as_slice());
        5
    } else {
        into[0] = 0xFF;
        let s = v.to_le_bytes();
        into[1..9].copy_from_slice(s.as_slice());
        9
    }
}
