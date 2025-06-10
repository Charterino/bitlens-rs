use anyhow::{Result, anyhow};
use bytes::BufMut;

use super::{packetpayload::SerializableValue, varint::VarInt};

// not necessarily valid UTF-8
pub type VarStr<'a> = Option<&'a [u8]>;

impl<'a> SerializableValue<'a> for VarStr<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(VarStr<'a>, usize)> {
        let (length, offset) = VarInt::deserialize(allocator, buffer)?;
        if length == 0 {
            return Ok((None, 1));
        }

        match buffer.get(offset..offset + length as usize) {
            Some(v) => Ok((Some(v), offset + length as usize)),
            None => Err(anyhow!("not enough bytes for varstr")),
        }
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        match self {
            Some(ua) => {
                let len = ua.len() as VarInt;
                len.serialize(stream);
                stream.put(*ua);
            }
            None => {
                stream.put_u8(0);
            }
        }
    }
}
