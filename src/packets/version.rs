use anyhow::Result;
use bytes::BufMut;
use tokio::io::AsyncReadExt;

use super::packetpayload::{Serializable, Stream};
use super::varstr::VarStr;
use super::{netaddr::NetAddrShort, packetpayload::PacketPayload};

#[derive(Default)]
pub struct Version<'a> {
    pub services: u64,
    pub timestamp: u64,
    pub addrrecv: NetAddrShort,
    pub addrfrom: NetAddrShort,
    pub nonce: u64,
    pub user_agent: VarStr<'a>,
    pub start_height: i32,
    pub version: i32,
    pub announce_relayed_transactions: bool,
}

pub const VERSION_COMMAND: [u8; 12] = *b"version\0\0\0\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for Version<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &VERSION_COMMAND
    }
}

impl<'a, 'b> Serializable<'a, 'b> for Version<'a> {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut impl Stream,
    ) -> Result<()> {
        self.version = stream.read_i32_le().await?;
        self.services = stream.read_u64_le().await?;
        self.timestamp = stream.read_u64_le().await?;
        self.addrrecv.deserialize(allocator, stream).await?;
        self.addrfrom.deserialize(allocator, stream).await?;
        self.nonce = stream.read_u64_le().await?;
        self.user_agent.deserialize(allocator, stream).await?;
        self.start_height = stream.read_i32_le().await?;
        if self.version >= 70001 {
            self.announce_relayed_transactions = stream.read_u8().await? != 0;
        }

        Ok(())
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        stream.put_i32_le(self.version);
        stream.put_u64_le(self.services);
        stream.put_u64_le(self.timestamp);
        self.addrrecv.serialize(stream);
        self.addrfrom.serialize(stream);
        stream.put_u64_le(self.nonce);
        self.user_agent.serialize(stream);
        stream.put_i32_le(self.start_height);
        if self.version >= 70001 {
            stream.put_u8(self.announce_relayed_transactions as u8);
        }
    }
}
