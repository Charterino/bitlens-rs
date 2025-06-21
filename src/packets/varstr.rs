use super::{
    deepclone::{DeepClone, MustOutlive},
    packetpayload::{Serializable, SerializableValue},
    varint::VarInt,
};
use anyhow::{Result, anyhow, bail};
use bytes::BufMut;
use supercow::Supercow;

// not necessarily valid UTF-8
#[derive(Debug, Clone)]
pub struct VarStr<'a> {
    pub inner: Supercow<'a, Vec<u8>, [u8]>,
}

impl Default for VarStr<'_> {
    fn default() -> Self {
        Self {
            inner: Supercow::owned(vec![]),
        }
    }
}

impl<'old> MustOutlive<'old> for VarStr<'old> {
    type WithLifetime<'new: 'old> = VarStr<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for VarStr<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let cloned = self.inner.as_ref().to_vec();
        Self::WithLifetime {
            inner: Supercow::owned(cloned),
        }
    }
}

impl<'a> Serializable<'a> for VarStr<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump,
        buffer: &'a [u8],
    ) -> Result<(Supercow<'a, VarStr<'a>>, usize)> {
        let (length, offset) = VarInt::deserialize(buffer)?;

        match buffer.get(offset..offset + length as usize) {
            Some(v) => match allocator.try_alloc(Self {
                inner: Supercow::borrowed(v),
            }) {
                Ok(v) => Ok((Supercow::borrowed(v), offset + length as usize)),
                Err(e) => bail!(e),
            },
            None => Err(anyhow!("not enough bytes for varstr")),
        }
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        let len = self.inner.len() as VarInt;
        len.serialize(stream);
        let r = self.inner.as_ref();
        stream.put(r);
    }
}

impl<'buffer, 'result: 'buffer> SerializableValue<'buffer> for VarStr<'result> {
    fn deserialize(buffer: &'buffer [u8]) -> Result<(VarStr<'result>, usize)> {
        let (length, offset) = VarInt::deserialize(buffer)?;

        match buffer.get(offset..offset + length as usize) {
            Some(v) => Ok((
                Self {
                    inner: Supercow::owned(v.to_vec()),
                },
                offset + length as usize,
            )),
            None => Err(anyhow!("not enough bytes for varstr")),
        }
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        let len = self.inner.len() as VarInt;
        len.serialize(stream);
        let r = self.inner.as_ref();
        stream.put(r);
    }
}

impl From<&str> for VarStr<'_> {
    fn from(value: &str) -> Self {
        Self {
            inner: Supercow::owned(value.as_bytes().to_vec()),
        }
    }
}
