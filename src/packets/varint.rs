use bytes::BufMut;
use tokio::{io::AsyncReadExt, net::tcp::ReadHalf};

use super::packetpayload::Serializable;
use anyhow::Result;

pub type VarInt = u64;

impl Serializable<'_, '_> for VarInt {
    async fn deserialize(
        &mut self,
        _allocator: &bumpalo::Bump<1>,
        stream: &'_ mut tokio::io::BufReader<ReadHalf<'_>>,
    ) -> Result<()> {
        let leader = stream.read_u8().await?;
        if leader < 0xFD {
            *self = leader as u64;
        } else if leader == 0xFD {
            *self = stream.read_u16_le().await? as u64;
        } else if leader == 0xFE {
            *self = stream.read_u32_le().await? as u64;
        } else {
            *self = stream.read_u64_le().await?;
        }

        Ok(())
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        if *self < 0xFD {
            stream.put_u8(*self as u8);
        } else if *self < 0xFFFF {
            stream.put_u8(0xFD);
            stream.put_u16_le(*self as u16);
        } else if *self < 0xFFFFFFFF {
            stream.put_u8(0xFE);
            stream.put_u32_le(*self as u32);
        } else {
            stream.put_u8(0xFF);
            stream.put_u64_le(*self);
        }
    }
}
