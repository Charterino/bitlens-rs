use super::{
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable},
};
use anyhow::bail;
use supercow::Supercow;

#[derive(Clone, Debug)]
pub struct SendHeaders {}

pub const SENDHEADERS_COMMAND: [u8; 12] = *b"sendheaders\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for SendHeaders {
    fn command(&self) -> &'static [u8; 12] {
        &SENDHEADERS_COMMAND
    }
}

impl<'old> MustOutlive<'old> for SendHeaders {
    type WithLifetime<'new: 'old> = SendHeaders;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for SendHeaders {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        Self::WithLifetime {}
    }
}

impl<'a> Serializable<'a> for SendHeaders {
    fn deserialize(
        a: &'a bumpalo::Bump<1>,
        _: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, SendHeaders>, usize)> {
        match a.try_alloc(SendHeaders {}) {
            Ok(v) => Ok((Supercow::borrowed(v), 0)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
