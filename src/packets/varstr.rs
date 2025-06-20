use std::borrow::Cow;

use anyhow::{Result, anyhow, bail};
use bytes::BufMut;

use super::{
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{Serializable, SerializableValue},
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

impl<'a> Serializable<'a> for VarStr<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump,
        buffer: &'a [u8],
    ) -> Result<(Cow<'a, VarStr<'a>>, usize)> {
        let (length, offset) = VarInt::deserialize(buffer)?;

        match buffer.get(offset..offset + length as usize) {
            Some(v) => match allocator.try_alloc(Self {
                inner: Cow::Borrowed(v),
            }) {
                Ok(v) => Ok((Cow::Borrowed(v), offset + length as usize)),
                Err(e) => bail!(e),
            },
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

impl<'buffer, 'result: 'buffer> SerializableValue<'buffer> for VarStr<'result> {
    fn deserialize(buffer: &'buffer [u8]) -> Result<(VarStr<'result>, usize)> {
        let (length, offset) = VarInt::deserialize(buffer)?;

        match buffer.get(offset..offset + length as usize) {
            Some(v) => Ok((
                Self {
                    inner: Cow::Owned(v.to_vec()),
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

impl From<&str> for VarStr<'_> {
    fn from(value: &str) -> Self {
        Self {
            inner: Cow::Owned(value.as_bytes().to_vec()),
        }
    }
}

impl<'a> VarStr<'a> {
    pub fn deserialize_copy(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &[u8],
    ) -> Result<(VarStr<'a>, usize)> {
        let (length, offset) = VarInt::deserialize(buffer)?;
        match buffer.get(offset..offset + length as usize) {
            Some(v) => {
                let copied = match allocator.try_alloc_slice_copy(v) {
                    Ok(c) => c,
                    Err(_) => bail!("oom"),
                };
                Ok((
                    Self {
                        inner: Cow::Borrowed(copied),
                    },
                    offset + length as usize,
                ))
            }
            None => Err(anyhow!("not enough bytes for varstr")),
        }
    }
}
