use super::{
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable},
};

#[derive(Default, Clone, Debug)]
pub struct Pong {
    pub nonce: u64,
}

pub const PONG_COMMAND: [u8; 12] = *b"pong\0\0\0\0\0\0\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for Pong {
    fn command(&self) -> &'static [u8; 12] {
        &PONG_COMMAND
    }
}

impl<'old> MustOutlive<'old> for Pong {
    type WithLifetime<'new: 'old> = Pong;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for Pong {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        Self::WithLifetime { nonce: self.nonce }
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
