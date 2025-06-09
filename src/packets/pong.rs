use super::{
    buffer::Buffer,
    packetpayload::{PacketPayload, Serializable},
};

#[derive(Default)]
pub struct Pong {
    pub nonce: u64,
}

pub const PONG_COMMAND: [u8; 12] = *b"pong\0\0\0\0\0\0\0\0";

impl PacketPayload<'_> for Pong {
    fn command(&self) -> &'static [u8; 12] {
        &PONG_COMMAND
    }
}

impl<'a> Serializable<'a> for Pong {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &[u8],
    ) -> anyhow::Result<(&'a Pong, usize)> {
        Ok((
            allocator.alloc(Pong {
                nonce: buffer.get_u64_le(0)?,
            }),
            8,
        ))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64(self.nonce);
    }
}
