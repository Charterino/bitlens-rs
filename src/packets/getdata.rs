use super::{
    SupercowVec,
    deepclone::{DeepClone, MustOutlive},
    invvector::InventoryVector,
    packetpayload::{PacketPayload, Serializable, SerializableSupercowVecOfCows},
};
use anyhow::bail;
use supercow::Supercow;

#[derive(Debug, Clone)]
pub struct GetData<'a> {
    pub inner: Supercow<'a, SupercowVec<'a, InventoryVector<'a>>>,
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
        let data = (&*self.inner.inner)
            .deep_clone()
            .into_iter()
            .map(Supercow::owned)
            .collect();
        Self::WithLifetime {
            inner: Supercow::owned(SupercowVec {
                inner: Supercow::owned(data),
            }),
        }
    }
}

impl<'a> Serializable<'a> for GetData<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, Self>, usize)> {
        let (deserialized, consumed) = SupercowVec::deserialize(allocator, buffer)?;
        match allocator.try_alloc(GetData {
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
