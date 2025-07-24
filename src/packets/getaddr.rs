use crate::util::arena::Arena;

use super::packetpayload::{DeserializableBorrowed, PacketPayload, Serializable};

#[derive(Clone, Debug, Copy, Default)]
pub struct GetAddr {}

pub const GETADDR_COMMAND: [u8; 12] = *b"getaddr\0\0\0\0\0";

impl PacketPayload<'_, GetAddr> for GetAddr {
    fn command(&self) -> &'static [u8; 12] {
        &GETADDR_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for GetAddr {
    fn deserialize_borrowed(&mut self, _: &'a Arena, _: &'a [u8]) -> anyhow::Result<usize> {
        Ok(0)
    }
}

impl Serializable for GetAddr {
    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
