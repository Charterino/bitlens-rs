use super::{
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable},
};
use anyhow::bail;
use supercow::Supercow;

#[derive(Default, Clone, Debug)]
pub struct GetAddr {}

pub const GETADDR_COMMAND: [u8; 12] = *b"getaddr\0\0\0\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for GetAddr {
    fn command(&self) -> &'static [u8; 12] {
        &GETADDR_COMMAND
    }
}

impl<'old> MustOutlive<'old> for GetAddr {
    type WithLifetime<'new: 'old> = GetAddr;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for GetAddr {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        Self::WithLifetime {}
    }
}

impl<'a> Serializable<'a> for GetAddr {
    fn deserialize(
        a: &'a bumpalo::Bump<1>,
        _: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, GetAddr>, usize)> {
        match a.try_alloc(GetAddr {}) {
            Ok(v) => Ok((Supercow::borrowed(v), 0)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, _: &mut impl bytes::BufMut) {}
}
