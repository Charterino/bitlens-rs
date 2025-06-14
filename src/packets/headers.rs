use std::borrow::Cow;

use anyhow::bail;

use super::{
    blockheader::BlockHeader,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{PacketPayload, Serializable, SerializableValue},
};

#[derive(Debug, Clone, Default)]
pub struct Headers<'a> {
    pub inner: Cow<'a, [Cow<'a, BlockHeader<'a>>]>,
}

pub const HEADERS_COMMAND: [u8; 12] = *b"headers\0\0\0\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for Headers<'old> {
    fn command(&self) -> &'static [u8; 12] {
        &HEADERS_COMMAND
    }
}

impl<'old> MustOutlive<'old> for Headers<'old> {
    type WithLifetime<'new: 'old> = Headers<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for Headers<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let addys = (&*self.inner)
            .deep_clone()
            .into_iter()
            .map(Cow::Owned)
            .collect();
        Self::WithLifetime { inner: addys }
    }
}

impl<'a> Serializable<'a> for Headers<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a Self, usize)> {
        let (deserialized, consumed) = Cow::deserialize(allocator, buffer)?;
        match allocator.try_alloc(Self {
            inner: deserialized,
        }) {
            Ok(result) => Ok((result, consumed)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        self.inner.serialize(stream);
    }
}
