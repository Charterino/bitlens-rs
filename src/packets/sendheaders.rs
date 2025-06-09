use super::packetpayload::{PacketPayload, Serializable};

#[derive(Default)]
pub struct SendHeaders {}

pub const SENDHEADERS_COMMAND: [u8; 12] = *b"sendheaders\0";

impl PacketPayload<'_> for SendHeaders {
    fn command(&self) -> &'static [u8; 12] {
        &SENDHEADERS_COMMAND
    }
}

impl<'a> Serializable<'a> for SendHeaders {
    fn deserialize(
        _: &'a bumpalo::Bump<1>,
        _: &'a [u8],
    ) -> anyhow::Result<(&'a SendHeaders, usize)> {
        Ok((&SendHeaders {}, 0))
    }

    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
