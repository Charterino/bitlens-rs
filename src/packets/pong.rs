use tokio::io::AsyncReadExt;

use super::packetpayload::{PacketPayload, Serializable, Stream};

#[derive(Default)]
pub struct Pong {
    pub nonce: u64,
}

pub const PONG_COMMAND: [u8; 12] = *b"pong\0\0\0\0\0\0\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for Pong {
    fn command(&self) -> &'static [u8; 12] {
        return &PONG_COMMAND;
    }
}

impl<'a, 'b> Serializable<'a, 'b> for Pong {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut impl Stream,
    ) -> anyhow::Result<()> {
        self.nonce = stream.read_u64().await?;
        Ok(())
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64(self.nonce);
    }
}
