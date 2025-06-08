use anyhow::bail;
use num::{FromPrimitive, ToPrimitive};
use num_derive::{FromPrimitive, ToPrimitive};
use tokio::io::AsyncReadExt;

use super::packetpayload::Serializable;

#[derive(FromPrimitive, ToPrimitive, Clone)]
pub enum InventoryVectorType {
    Error = 0,
    Tx = 1,
    Block = 2,
    FilteredBlock = 3,
    CmpctBlock = 4,
    WitnessTx = 0x40000001,
    WitnessBlock = 0x40000002,
    FilteredWitnessBlock = 0x40000003,
}

impl Default for InventoryVectorType {
    fn default() -> Self {
        InventoryVectorType::Error
    }
}

#[derive(Default, Clone)]
pub struct InventoryVector {
    pub inv_type: InventoryVectorType,
    pub hash: [u8; 32],
}

impl Serializable<'_, '_> for InventoryVector {
    async fn deserialize(
        &mut self,
        allocator: &'_ bumpalo::Bump<1>,
        stream: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'_>>,
    ) -> anyhow::Result<()> {
        let raw_type = stream.read_u32_le().await?;
        match InventoryVectorType::from_u32(raw_type) {
            Some(inv_type) => {
                self.inv_type = inv_type;
            }
            None => {
                bail!("unknown inventory type: {:x}", raw_type)
            }
        }
        stream.read_exact(&mut self.hash).await?;
        Ok(())
    }

    fn serialize(&self, stream: &mut impl bytes::BufMut) {
        stream.put_u32_le(self.inv_type.to_u32().unwrap());
        stream.put(self.hash.as_slice());
    }
}
