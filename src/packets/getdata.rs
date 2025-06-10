use std::borrow::Cow;

use super::{
    inv::InventoryVector,
    packetpayload::{PacketPayload, Serializable, SerializableValue},
};

#[derive(Debug, Clone, Default)]
pub struct GetData<'a> {
    pub inner: Cow<'a, [Cow<'a, InventoryVector<'a>>]>,
}

pub const GETDATA_COMMAND: [u8; 12] = *b"getdata\0\0\0\0\0";

impl<'a> PacketPayload<'a> for GetData<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &GETDATA_COMMAND
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
