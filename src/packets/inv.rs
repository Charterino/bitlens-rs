use super::{
    SupercowVec,
    deepclone::{DeepClone, MustOutlive},
    invvector::InventoryVector,
    packetpayload::{PacketPayload, Serializable, SerializableSupercowVecOfCows},
};
use anyhow::bail;
use supercow::Supercow;

#[derive(Clone, Debug)]
pub struct Inv<'a> {
    pub inner: Supercow<'a, SupercowVec<'a, InventoryVector<'a>>>,
}

impl Default for Inv<'_> {
    fn default() -> Self {
        Self {
            inner: Supercow::owned(Default::default()),
        }
    }
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
        let invs: Vec<Supercow<InventoryVector>> = (&*self.inner.inner)
            .deep_clone()
            .into_iter()
            .map(Supercow::owned)
            .collect();
        Self::WithLifetime {
            inner: Supercow::owned(SupercowVec {
                inner: Supercow::owned(invs),
            }),
        }
    }
}

impl<'a> Serializable<'a> for Inv<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, Self>, usize)> {
        let (deserialized, consumed) = SupercowVec::deserialize(allocator, buffer)?;
        match allocator.try_alloc(Inv {
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
