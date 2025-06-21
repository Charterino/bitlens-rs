use super::{
    Array,
    deepclone::{DeepClone, MustOutlive},
    invvector::InventoryVector,
    packetpayload::{PacketPayload, Serializable, SerializableArrayOfCows},
};
use anyhow::bail;
use supercow::Supercow;

#[derive(Clone, Debug, Default)]
pub struct Inv<'a> {
    //pub inner: Supercow<'a, std::vec::Vec<InventoryVector<'a>>, [InventoryVector<'a>]>,
    pub inner: Array<'a, InventoryVector<'a>>,
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
            inner: Array {
                inner: Supercow::owned(invs),
            },
        }
    }
}

impl<'a> Serializable<'a> for Inv<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, Self>, usize)> {
        let (deserialized, consumed) = Array::deserialize(allocator, buffer)?;
        match allocator.try_alloc(Inv {
            inner: deserialized,
        }) {
            Ok(result) => Ok((Supercow::borrowed(result), consumed)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        self.inner.serialize(stream);
    }
}
