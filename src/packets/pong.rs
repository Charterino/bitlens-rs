use crate::util::arena::Arena;

use super::{
    buffer::Buffer,
    packetpayload::{DeserializableBorrowed, PacketPayload, Serializable},
};
use anyhow::Result;

#[derive(Clone, Copy, Default, Debug)]
pub struct Pong {
    pub nonce: u64,
}

pub const PONG_COMMAND: [u8; 12] = *b"pong\0\0\0\0\0\0\0\0";

impl PacketPayload<'_, Pong> for Pong {
    fn command(&self) -> &'static [u8; 12] {
        &PONG_COMMAND
    }
}

impl DeserializableBorrowed<'_> for Pong {
    fn deserialize_borrowed(&mut self, _: &Arena, buffer: &[u8]) -> Result<usize> {
        self.nonce = buffer.get_u64_le(0)?;
        Ok(8)
    }
}

impl Serializable for Pong {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64(self.nonce);
    }
}
