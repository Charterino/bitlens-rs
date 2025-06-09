use super::packetpayload::{PacketPayload, Serializable};

#[derive(Default)]
pub struct VerAck {}

pub const VERACK_COMMAND: [u8; 12] = *b"verack\0\0\0\0\0\0";

impl PacketPayload<'_> for VerAck {
    fn command(&self) -> &'static [u8; 12] {
        &VERACK_COMMAND
    }
}

impl<'a> Serializable<'a> for VerAck {
    fn deserialize(_: &'a bumpalo::Bump<1>, _: &'a [u8]) -> anyhow::Result<(&'a VerAck, usize)> {
        Ok((&VerAck {}, 0))
    }

    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
