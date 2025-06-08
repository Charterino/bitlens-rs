use super::packetpayload::{PacketPayload, Serializable};

#[derive(Default)]
pub struct VerAck {}

pub const VERACK_COMMAND: [u8; 12] = *b"verack\0\0\0\0\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for VerAck {
    fn command(&self) -> &'static [u8; 12] {
        return &VERACK_COMMAND;
    }
}

impl<'a, 'b> Serializable<'a, 'b> for VerAck {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'b>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {}
}
