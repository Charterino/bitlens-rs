use super::packetpayload::{PacketPayload, Serializable};

#[derive(Default)]
pub struct SendHeaders {}

pub const SENDHEADERS_COMMAND: [u8; 12] = *b"sendheaders\0";

impl<'a, 'b> PacketPayload<'a, 'b> for SendHeaders {
    fn command(&self) -> &'static [u8; 12] {
        return &SENDHEADERS_COMMAND;
    }
}

impl<'a, 'b> Serializable<'a, 'b> for SendHeaders {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'b>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {}
}
