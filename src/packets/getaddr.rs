
use super::packetpayload::{PacketPayload, Serializable, Stream};

#[derive(Default)]
pub struct GetAddr {}

pub const GETADDR_COMMAND: [u8; 12] = *b"getaddr\0\0\0\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for GetAddr {
    fn command(&self) -> &'static [u8; 12] {
        return &GETADDR_COMMAND;
    }
}

impl<'a, 'b> Serializable<'a, 'b> for GetAddr {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut impl Stream,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {}
}
