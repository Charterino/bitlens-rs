use std::borrow::Cow;

use super::{
    netaddr::NetAddr,
    packetpayload::{PacketPayload, Serializable, SerializableValue},
};

#[derive(Debug, Clone, Default)]
pub struct Addr<'a> {
    pub inner: Cow<'a, [Cow<'a, NetAddr<'a>>]>,
}

pub const ADDR_COMMAND: [u8; 12] = *b"addr\0\0\0\0\0\0\0\0";

impl<'a> PacketPayload<'a> for Addr<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &ADDR_COMMAND
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
