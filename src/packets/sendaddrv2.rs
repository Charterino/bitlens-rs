use std::borrow::Cow;

use anyhow::bail;

use super::{
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable},
};

#[derive(Default, Debug, Clone)]
pub struct SendAddrV2 {}

pub const SENDADDRV2_COMMAND: [u8; 12] = *b"sendaddrv2\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for SendAddrV2 {
    fn command(&self) -> &'static [u8; 12] {
        &SENDADDRV2_COMMAND
    }
}

impl<'old> MustOutlive<'old> for SendAddrV2 {
    type WithLifetime<'new: 'old> = SendAddrV2;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for SendAddrV2 {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        Self::WithLifetime {}
    }
}

impl<'a> Serializable<'a> for SendAddrV2 {
    fn deserialize(
        a: &'a bumpalo::Bump<1>,
        _: &'a [u8],
    ) -> anyhow::Result<(Cow<'a, SendAddrV2>, usize)> {
        match a.try_alloc(SendAddrV2 {}) {
            Ok(v) => Ok((Cow::Borrowed(v), 0)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
