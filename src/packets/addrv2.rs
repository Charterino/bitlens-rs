use super::{
    SupercowVec,
    deepclone::{DeepClone, MustOutlive},
    netaddr::NetAddrV2,
    packetpayload::{PacketPayload, Serializable, SerializableSupercowVecOfCows},
};
use anyhow::bail;
use supercow::Supercow;

#[derive(Debug, Clone)]
pub struct AddrV2<'a> {
    pub inner: Supercow<'a, SupercowVec<'a, NetAddrV2<'a>>>,
}

pub const ADDRV2_COMMAND: [u8; 12] = *b"addrv2\0\0\0\0\0\0";

impl<'old, 'new: 'old> PacketPayload<'old, 'new> for AddrV2<'old> {
    fn command(&self) -> &'static [u8; 12] {
        &ADDRV2_COMMAND
    }
}

impl<'old> MustOutlive<'old> for AddrV2<'old> {
    type WithLifetime<'new: 'old> = AddrV2<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for AddrV2<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let addys = (&*self.inner.inner)
            .deep_clone()
            .into_iter()
            .map(Supercow::owned)
            .collect();
        Self::WithLifetime {
            inner: Supercow::owned(SupercowVec {
                inner: Supercow::owned(addys),
            }),
        }
    }
}

impl<'a> Serializable<'a> for AddrV2<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, Self>, usize)> {
        let (deserialized, consumed) = SupercowVec::deserialize(allocator, buffer)?;
        match allocator.try_alloc(AddrV2 {
            inner: Supercow::borrowed(deserialized),
        }) {
            Ok(result) => Ok((Supercow::borrowed(result), consumed)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        self.inner.serialize(stream);
    }
}
