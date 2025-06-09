use super::{
    buffer::Buffer,
    packetpayload::{PacketPayload, Serializable},
};

#[derive(Default)]
pub struct Ping {
    pub nonce: u64,
}

pub const PING_COMMAND: [u8; 12] = *b"ping\0\0\0\0\0\0\0\0";

impl PacketPayload<'_> for Ping {
    fn command(&self) -> &'static [u8; 12] {
        &PING_COMMAND
    }
}

impl<'a> Serializable<'a> for Ping {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &[u8],
    ) -> anyhow::Result<(&'a Ping, usize)> {
        Ok((
            allocator.alloc(Ping {
                nonce: buffer.get_u64_le(0)?,
            }),
            8,
        ))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64(self.nonce);
    }
}
