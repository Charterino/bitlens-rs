use crate::util::arena::Arena;

use super::{
    EMPTY_HASH,
    buffer::Buffer,
    packetpayload::{DeserializableBorrowed, Serializable},
};
use anyhow::bail;
use num::{FromPrimitive, ToPrimitive};
use num_derive::{FromPrimitive, ToPrimitive};

#[derive(FromPrimitive, ToPrimitive, Clone, Default, Debug, Copy, PartialEq, Eq)]
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

#[derive(Clone, Debug, Copy)]
pub struct InventoryVectorBorrowed<'a> {
    pub inv_type: InventoryVectorType,
    pub hash: &'a [u8; 32],
}

impl Default for InventoryVectorBorrowed<'_> {
    fn default() -> Self {
        Self {
            inv_type: Default::default(),
            hash: &EMPTY_HASH,
        }
    }
}

impl<'a> DeserializableBorrowed<'a> for InventoryVectorBorrowed<'a> {
    fn deserialize_borrowed(&mut self, _: &'a Arena, buffer: &'a [u8]) -> anyhow::Result<usize> {
        let raw_type = buffer.get_u32_le(0)?;
        self.inv_type = match InventoryVectorType::from_u32(raw_type) {
            Some(inv_type) => inv_type,
            None => {
                bail!("unknown inventory type: {:x}", raw_type)
            }
        };
        self.hash = buffer.get_hash(4)?;
        Ok(36)
    }
}

#[derive(Clone, Debug, Copy, Default)]
pub struct InventoryVectorOwned {
    pub inv_type: InventoryVectorType,
    pub hash: [u8; 32],
}

impl Serializable for InventoryVectorOwned {
    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.inv_type.to_u32().unwrap());
        stream.put(self.hash.as_slice());
    }
}

impl From<InventoryVectorBorrowed<'_>> for InventoryVectorOwned {
    fn from(value: InventoryVectorBorrowed<'_>) -> Self {
        InventoryVectorOwned {
            inv_type: value.inv_type,
            hash: *value.hash,
        }
    }
}
