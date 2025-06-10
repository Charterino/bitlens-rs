use std::borrow::Cow;

use super::{
    netaddr::NetAddrV2,
    packetpayload::{PacketPayload, Serializable, SerializableValue},
};

#[derive(Debug, Clone, Default)]
pub struct AddrV2<'a> {
    pub inner: Cow<'a, [Cow<'a, NetAddrV2<'a>>]>,
}

pub const ADDRV2_COMMAND: [u8; 12] = *b"addrv2\0\0\0\0\0\0";

impl<'a> PacketPayload<'a> for AddrV2<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &ADDRV2_COMMAND
    }
}

impl<'a> Serializable<'a> for AddrV2<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a Self, usize)> {
        let (deserialized, consumed) = Cow::deserialize(allocator, buffer)?;
        Ok((
            allocator.alloc(AddrV2 {
                inner: deserialized,
            }),
            consumed,
        ))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        self.inner.serialize(stream);
    }
}
