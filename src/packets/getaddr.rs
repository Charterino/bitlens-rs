use super::packetpayload::{PacketPayload, Serializable};

#[derive(Default)]
pub struct GetAddr {}

pub const GETADDR_COMMAND: [u8; 12] = *b"getaddr\0\0\0\0\0";

impl PacketPayload<'_> for GetAddr {
    fn command(&self) -> &'static [u8; 12] {
        &GETADDR_COMMAND
    }
}

impl<'a> Serializable<'a> for GetAddr {
    fn deserialize(_: &'a bumpalo::Bump<1>, _: &'a [u8]) -> anyhow::Result<(&'a GetAddr, usize)> {
        Ok((&GetAddr {}, 0))
    }

    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
