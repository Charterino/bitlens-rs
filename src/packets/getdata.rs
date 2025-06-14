use std::borrow::Cow;

use super::{
    deepclone::{DeepClone, MustOutlive},
    invvector::InventoryVector,
    packetpayload::{PacketPayload, Serializable, SerializableValue},
};

#[derive(Debug, Clone, Default)]
pub struct GetData<'a> {
    pub inner: Cow<'a, [Cow<'a, InventoryVector<'a>>]>,
}

pub const GETDATA_COMMAND: [u8; 12] = *b"getdata\0\0\0\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for GetData<'old> {
    fn command(&self) -> &'static [u8; 12] {
        &GETDATA_COMMAND
    }
}

impl<'old> MustOutlive<'old> for GetData<'old> {
    type WithLifetime<'new: 'old> = GetData<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for GetData<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let data = (&*self.inner)
            .deep_clone()
            .into_iter()
            .map(Cow::Owned)
            .collect();
        Self::WithLifetime { inner: data }
    }
}

impl<'a> Serializable<'a> for GetData<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a Self, usize)> {
        let (deserialized, consumed) = Cow::deserialize(allocator, buffer)?;
        Ok((
            allocator.alloc(GetData {
                inner: deserialized,
            }),
            consumed,
        ))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        self.inner.serialize(stream);
    }
}
