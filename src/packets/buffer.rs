use anyhow::{Result, anyhow};

pub trait Buffer<'a> {
    fn get_hash(self, offset: usize) -> Result<&'a [u8; 32]>;
    fn get_u64(self, offset: usize) -> Result<u64>;
    fn get_u64_le(self, offset: usize) -> Result<u64>;
    fn get_u32(self, offset: usize) -> Result<u32>;
    fn get_u32_le(self, offset: usize) -> Result<u32>;
    fn get_u16(self, offset: usize) -> Result<u16>;
    fn get_u16_le(self, offset: usize) -> Result<u16>;
    fn with_offset(self, offset: usize) -> Result<&'a [u8]>;
}

impl<'a> Buffer<'a> for &'a [u8] {
    fn get_hash(self, offset: usize) -> Result<&'a [u8; 32]> {
        match self.get(offset..offset + 32) {
            Some(r) => Ok(r.try_into()?),
            None => Err(anyhow!("not enough bytes for a 32 byte hash")),
        }
    }

    fn with_offset(self, offset: usize) -> Result<&'a [u8]> {
        match self.get(offset..) {
            Some(v) => Ok(v),
            None => Err(anyhow!("not enough bytes for with_offset")),
        }
    }

    fn get_u32_le(self, offset: usize) -> Result<u32> {
        match self.get(offset..offset + 4) {
            Some(b) => Ok(u32::from_le_bytes(b.try_into()?)),
            None => Err(anyhow!("not enough bytes for a 32 bit integer")),
        }
    }

    fn get_u64(self, offset: usize) -> Result<u64> {
        match self.get(offset..offset + 8) {
            Some(b) => Ok(u64::from_be_bytes(b.try_into()?)),
            None => Err(anyhow!("not enough bytes for a 64 bit integer")),
        }
    }

    fn get_u64_le(self, offset: usize) -> Result<u64> {
        match self.get(offset..offset + 8) {
            Some(b) => Ok(u64::from_le_bytes(b.try_into()?)),
            None => Err(anyhow!("not enough bytes for a 64 bit integer")),
        }
    }

    fn get_u32(self, offset: usize) -> Result<u32> {
        match self.get(offset..offset + 4) {
            Some(b) => Ok(u32::from_be_bytes(b.try_into()?)),
            None => Err(anyhow!("not enough bytes for a 32 bit integer")),
        }
    }

    fn get_u16(self, offset: usize) -> Result<u16> {
        match self.get(offset..offset + 2) {
            Some(b) => Ok(u16::from_be_bytes(b.try_into()?)),
            None => Err(anyhow!("not enough bytes for a 16 bit integer")),
        }
    }

    fn get_u16_le(self, offset: usize) -> Result<u16> {
        match self.get(offset..offset + 2) {
            Some(b) => Ok(u16::from_le_bytes(b.try_into()?)),
            None => Err(anyhow!("not enough bytes for a 16 bit integer")),
        }
    }
}
