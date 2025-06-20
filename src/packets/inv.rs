use std::borrow::Cow;

use anyhow::bail;

use super::{
    deepclone::{DeepClone, MustOutlive},
    invvector::InventoryVector,
    packetpayload::{PacketPayload, Serializable, SerializableArrayOfCows},
};

#[derive(Debug, Clone, Default)]
pub struct Inv<'a> {
    pub inner: Cow<'a, [Cow<'a, InventoryVector<'a>>]>,
}

pub const INV_COMMAND: [u8; 12] = *b"inv\0\0\0\0\0\0\0\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for Inv<'old> {
    fn command(&self) -> &'static [u8; 12] {
        &INV_COMMAND
    }
}

impl<'old> MustOutlive<'old> for Inv<'old> {
    type WithLifetime<'new: 'old> = Inv<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for Inv<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let invs = (&*self.inner)
            .deep_clone()
            .into_iter()
            .map(Cow::Owned)
            .collect();
        Self::WithLifetime { inner: invs }
    }
}

impl<'a> Serializable<'a> for Inv<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(Cow<'a, Self>, usize)> {
        let (deserialized, consumed) = Cow::deserialize(allocator, buffer)?;
        match allocator.try_alloc(Inv {
            inner: deserialized,
        }) {
            Ok(result) => Ok((Cow::Borrowed(result), consumed)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        self.inner.serialize(stream);
    }
}
