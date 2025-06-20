use std::borrow::Cow;

use anyhow::bail;

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
    fn deserialize(
        a: &'a bumpalo::Bump<1>,
        _: &'a [u8],
    ) -> anyhow::Result<(Cow<'a, VerAck>, usize)> {
        match a.try_alloc(VerAck {}) {
            Ok(v) => Ok((Cow::Borrowed(v), 0)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
