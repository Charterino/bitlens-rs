use anyhow::bail;
use num::{FromPrimitive, ToPrimitive};
use num_derive::{FromPrimitive, ToPrimitive};

use super::{buffer::Buffer, packetpayload::Serializable};

#[derive(FromPrimitive, ToPrimitive, Clone, Default)]
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

pub struct InventoryVector<'a> {
    pub inv_type: InventoryVectorType,
    pub hash: &'a [u8; 32],
}

impl<'a> Serializable<'a> for InventoryVector<'a> {
    fn deserialize(
        allocator: &'a bumpalo::Bump<1>,
        buffer: &'a [u8],
    ) -> anyhow::Result<(&'a InventoryVector<'a>, usize)> {
        let raw_type = buffer.get_u32_le(0)?;
        let inv_type = match InventoryVectorType::from_u32(raw_type) {
            Some(inv_type) => inv_type,
            None => {
                bail!("unknown inventory type: {:x}", raw_type)
            }
        };
        let hash = buffer.get_hash(4)?;
        Ok((allocator.alloc(InventoryVector { inv_type, hash }), 36))
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.inv_type.to_u32().unwrap());
        stream.put(self.hash.as_slice());
    }
}
