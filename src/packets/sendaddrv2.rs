use super::packetpayload::{PacketPayload, Serializable};

#[derive(Default)]
pub struct SendAddrV2 {}

pub const SENDADDRV2_COMMAND: [u8; 12] = *b"sendaddrv2\0\0";

impl<'a> PacketPayload<'a> for SendAddrV2 {
    fn command(&self) -> &'static [u8; 12] {
        return &SENDADDRV2_COMMAND;
    }
}

impl<'a> Serializable<'a> for SendAddrV2 {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a SendAddrV2, usize)> {
        Ok((&SendAddrV2 {}, 0))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {}
}
