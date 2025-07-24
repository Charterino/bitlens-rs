use crate::util::arena::Arena;

use super::packetpayload::{DeserializableBorrowed, PacketPayload, Serializable};

#[derive(Debug, Clone, Copy, Default)]
pub struct SendAddrV2 {}

pub const SENDADDRV2_COMMAND: [u8; 12] = *b"sendaddrv2\0\0";

impl PacketPayload<'_, SendAddrV2> for SendAddrV2 {
    fn command(&self) -> &'static [u8; 12] {
        &SENDADDRV2_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for SendAddrV2 {
    fn deserialize_borrowed(&mut self, _: &'a Arena, _: &'a [u8]) -> anyhow::Result<usize> {
        Ok(0)
    }
}

impl Serializable for SendAddrV2 {
    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
