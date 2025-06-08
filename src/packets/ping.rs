use tokio::io::AsyncReadExt;

use super::packetpayload::{PacketPayload, Serializable, Stream};

#[derive(Default)]
pub struct Ping {
    pub nonce: u64,
}

pub const PING_COMMAND: [u8; 12] = *b"ping\0\0\0\0\0\0\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for Ping {
    fn command(&self) -> &'static [u8; 12] {
        return &PING_COMMAND;
    }
}

impl<'a, 'b> Serializable<'a, 'b> for Ping {
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
