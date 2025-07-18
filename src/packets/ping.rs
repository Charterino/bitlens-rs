use super::{
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable},
};
use anyhow::bail;
use supercow::Supercow;

#[derive(Clone, Debug)]
pub struct Ping {
    pub nonce: u64,
}

pub const PING_COMMAND: [u8; 12] = *b"ping\0\0\0\0\0\0\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for Ping {
    fn command(&self) -> &'static [u8; 12] {
        &PING_COMMAND
    }
}

impl<'old> MustOutlive<'old> for Ping {
    type WithLifetime<'new: 'old> = Ping;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for Ping {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        Self::WithLifetime { nonce: self.nonce }
    }
}

impl<'a> Serializable<'a> for Ping {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &[u8],
    ) -> anyhow::Result<(Supercow<'a, Ping>, usize)> {
        match allocator.try_alloc(Ping {
            nonce: buffer.get_u64_le(0)?,
        }) {
            Ok(result) => Ok((Supercow::borrowed(result), 8)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u64(self.nonce);
    }
}
