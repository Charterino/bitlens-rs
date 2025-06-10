use super::{
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable},
};

#[derive(Default, Clone, Debug)]
pub struct VerAck {}

pub const VERACK_COMMAND: [u8; 12] = *b"verack\0\0\0\0\0\0";

impl<'old> MustOutlive<'old> for VerAck {
    type WithLifetime<'new: 'old> = VerAck;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for VerAck {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        Self::WithLifetime {}
    }
}

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for VerAck {
    fn command(&self) -> &'static [u8; 12] {
        &VERACK_COMMAND
    }
}

impl<'a> Serializable<'a> for VerAck {
    fn deserialize(_: &'a bumpalo::Bump<1>, _: &'a [u8]) -> anyhow::Result<(&'a VerAck, usize)> {
        Ok((&VerAck {}, 0))
    }

    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
