use super::packetpayload::{PacketPayload, Serializable};

#[derive(Default)]
pub struct SendAddrV2 {}

pub const SENDADDRV2_COMMAND: [u8; 12] = *b"sendaddrv2\0\0";

impl PacketPayload<'_> for SendAddrV2 {
    fn command(&self) -> &'static [u8; 12] {
        &SENDADDRV2_COMMAND
    }
}

impl<'a> Serializable<'a> for SendAddrV2 {
    fn deserialize(
        _: &'a bumpalo::Bump<1>,
        _: &'a [u8],
    ) -> anyhow::Result<(&'a SendAddrV2, usize)> {
        Ok((&SendAddrV2 {}, 0))
    }

    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
