use std::borrow::Cow;

use anyhow::{Result, anyhow};
use bytes::BufMut;

use super::{
    deepclone::{DeepClone, MustOutlive},
    packetpayload::SerializableValue,
    varint::VarInt,
};

// not necessarily valid UTF-8
#[derive(Debug, Clone, Default)]
pub struct VarStr<'a> {
    pub inner: Cow<'a, [u8]>,
}

impl<'old> MustOutlive<'old> for VarStr<'old> {
    type WithLifetime<'new: 'old> = VarStr<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for VarStr<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let cloned = self.inner.clone().into_owned();
        Self::WithLifetime {
            inner: Cow::Owned(cloned),
        }
    }
}

impl<'a> SerializableValue<'a> for VarStr<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> Result<(VarStr<'a>, usize)> {
        let (length, offset) = VarInt::deserialize(allocator, buffer)?;

        match buffer.get(offset..offset + length as usize) {
            Some(v) => Ok((
                Self {
                    inner: Cow::Borrowed(v),
                },
                offset + length as usize,
            )),
            None => Err(anyhow!("not enough bytes for varstr")),
        }
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        let len = self.inner.len() as VarInt;
        len.serialize(stream);
        match &self.inner {
            Cow::Borrowed(ua) => stream.put(*ua),
            Cow::Owned(ua) => stream.put(ua.as_slice()),
        }
    }
}
