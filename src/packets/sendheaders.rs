use crate::util::arena::Arena;

use super::packetpayload::{DeserializableBorrowed, PacketPayload, Serializable};

#[derive(Clone, Debug, Copy, Default)]
pub struct SendHeaders {}

pub const SENDHEADERS_COMMAND: [u8; 12] = *b"sendheaders\0";

impl PacketPayload<'_, SendHeaders> for SendHeaders {
    fn command(&self) -> &'static [u8; 12] {
        &SENDHEADERS_COMMAND
    }
}

impl<'a> DeserializableBorrowed<'a> for SendHeaders {
    fn deserialize_borrowed(&mut self, _: &'a Arena, _: &'a [u8]) -> anyhow::Result<usize> {
        Ok(0)
    }
}

impl Serializable for SendHeaders {
    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
