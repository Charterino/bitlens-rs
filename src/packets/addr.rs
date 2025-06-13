use std::borrow::Cow;

use super::{
    deepclone::{DeepClone, MustOutlive},
    netaddr::NetAddr,
    packetpayload::{PacketPayload, Serializable, SerializableValue},
};

#[derive(Debug, Clone, Default)]
pub struct Addr<'a> {
    pub inner: Cow<'a, [Cow<'a, NetAddr<'a>>]>,
}

pub const ADDR_COMMAND: [u8; 12] = *b"addr\0\0\0\0\0\0\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for Addr<'old> {
    fn command(&self) -> &'static [u8; 12] {
        &ADDR_COMMAND
    }
}

impl<'old> MustOutlive<'old> for Addr<'old> {
    type WithLifetime<'new: 'old> = Addr<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for Addr<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let addys = (&*self.inner)
            .deep_clone()
            .into_iter()
            .map(Cow::Owned)
            .collect();
        Self::WithLifetime { inner: addys }
    }
}

impl<'a> Serializable<'a> for Addr<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a Self, usize)> {
        let (deserialized, consumed) = Cow::deserialize(allocator, buffer)?;
        Ok((
            allocator.alloc(Addr {
                inner: deserialized,
            }),
            consumed,
        ))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        self.inner.serialize(stream);
    }
}
