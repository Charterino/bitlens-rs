use crate::util::arena::Arena;

use super::{
    buffer::Buffer,
    deepclone::{DeepClone, MustOutlive},
    packetpayload::Serializable,
};
use anyhow::bail;
use num::{FromPrimitive, ToPrimitive};
use num_derive::{FromPrimitive, ToPrimitive};
use supercow::Supercow;

#[derive(FromPrimitive, ToPrimitive, Clone, Default, Debug)]
pub enum InventoryVectorType {
    #[default]
    Error = 0,
    Tx = 1,
    Block = 2,
    FilteredBlock = 3,
    CmpctBlock = 4,
    WitnessTx = 0x40000001,
    WitnessBlock = 0x40000002,
    FilteredWitnessBlock = 0x40000003,
}

#[derive(Clone, Debug)]
pub struct InventoryVector<'a> {
    pub inv_type: InventoryVectorType,
    pub hash: Supercow<'a, [u8; 32]>,
}

impl<'a> Serializable<'a> for InventoryVector<'a> {
    fn deserialize(
        allocator: &'a Arena,
        buffer: &'a [u8],
    ) -> anyhow::Result<(Supercow<'a, InventoryVector<'a>>, usize)> {
        let raw_type = buffer.get_u32_le(0)?;
        let inv_type = match InventoryVectorType::from_u32(raw_type) {
            Some(inv_type) => inv_type,
            None => {
                bail!("unknown inventory type: {:x}", raw_type)
            }
        };
        let hash = buffer.get_hash(4)?;
        match allocator.try_alloc(InventoryVector {
            inv_type,
            hash: Supercow::borrowed(hash),
        }) {
            Ok(result) => Ok((Supercow::borrowed(result), 36)),
            Err(e) => bail!(e),
        }
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.inv_type.to_u32().unwrap());
        stream.put(self.hash.as_slice());
    }
}

impl<'old> MustOutlive<'old> for InventoryVector<'old> {
    type WithLifetime<'new: 'old> = InventoryVector<'new>;
}

impl<'old, 'new: 'old> DeepClone<'old, 'new> for InventoryVector<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let hash = *self.hash;
        Self::WithLifetime {
            inv_type: self.inv_type.clone(),
            hash: Supercow::owned(hash),
        }
    }
}
