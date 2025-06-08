use super::packetpayload::{PacketPayload, Serializable};

#[derive(Default)]
pub struct SendAddrV2 {}

pub const SENDADDRV2_COMMAND: [u8; 12] = *b"sendaddrv2\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for SendAddrV2 {
    fn command(&self) -> &'static [u8; 12] {
        return &SENDADDRV2_COMMAND;
    }
}

impl<'a, 'b> Serializable<'a, 'b> for SendAddrV2 {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'b>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {}
}
