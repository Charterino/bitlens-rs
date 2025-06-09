use super::packetpayload::{PacketPayload, Serializable};

#[derive(Default)]
pub struct SendHeaders {}

pub const SENDHEADERS_COMMAND: [u8; 12] = *b"sendheaders\0";

impl<'a> PacketPayload<'a> for SendHeaders {
    fn command(&self) -> &'static [u8; 12] {
        return &SENDHEADERS_COMMAND;
    }
}

impl<'a> Serializable<'a> for SendHeaders {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a SendHeaders, usize)> {
        Ok((&SendHeaders {}, 0))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {}
}
