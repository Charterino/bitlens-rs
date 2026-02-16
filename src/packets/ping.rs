use crate::util::arena::Arena;

use super::{
    buffer::Buffer,
    packetpayload::{DeserializableBorrowed, PacketPayload, Serializable},
};
use anyhow::Result;

#[derive(Clone, Copy, Debug, Default)]
pub struct Ping {
    pub nonce: u64,
}

pub const PING_COMMAND: [u8; 12] = *b"ping\0\0\0\0\0\0\0\0";

impl PacketPayload<'_, Ping> for Ping {
    fn command(&self) -> &'static [u8; 12] {
        &PING_COMMAND
    }
}

impl DeserializableBorrowed<'_> for Ping {
    fn deserialize_borrowed(&mut self, _: &Arena, buffer: &[u8]) -> Result<usize> {
        self.nonce = buffer.get_u64_le(0)?;
        Ok(8)
    }
}

impl Serializable for Ping {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64_le(self.nonce);
    }
}
