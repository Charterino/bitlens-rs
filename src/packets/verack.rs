use super::packetpayload::{PacketPayload, Serializable};

#[derive(Default)]
pub struct VerAck {}

pub const VERACK_COMMAND: [u8; 12] = *b"verack\0\0\0\0\0\0";

impl<'a> PacketPayload<'a> for VerAck {
    fn command(&self) -> &'static [u8; 12] {
        return &VERACK_COMMAND;
    }
}

impl<'a> Serializable<'a> for VerAck {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a VerAck, usize)> {
        Ok((&VerAck {}, 0))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {}
}
