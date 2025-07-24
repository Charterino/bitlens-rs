use super::packetpayload::{DeserializableBorrowed, PacketPayload, Serializable};
use crate::util::arena::Arena;
use anyhow::Result;

#[derive(Clone, Copy, Default, Debug)]
pub struct VerAck {}

pub const VERACK_COMMAND: [u8; 12] = *b"verack\0\0\0\0\0\0";

impl PacketPayload<'_, VerAck> for VerAck {
    fn command(&self) -> &'static [u8; 12] {
        &VERACK_COMMAND
    }
}

impl DeserializableBorrowed<'_> for VerAck {
    fn deserialize_borrowed(&mut self, _: &Arena, _: &[u8]) -> Result<usize> {
        Ok(0)
    }
}

impl Serializable for VerAck {
    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
