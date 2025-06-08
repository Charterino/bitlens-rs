use anyhow::{Result, bail};
use bytes::BufMut;
use tokio::{io::AsyncReadExt, net::tcp::ReadHalf};

use super::packetpayload::Serializable;

#[derive(Default)]
pub struct NetAddrShort {
    pub services: u64,
    pub addr: [u8; 16],
    pub port: u16,
}

impl Serializable<'_, '_> for NetAddrShort {
    async fn deserialize(
        &mut self,
        _allocator: &bumpalo::Bump<1>,
        stream: &mut tokio::io::BufReader<ReadHalf<'_>>,
    ) -> Result<()> {
        self.services = stream.read_u64_le().await?;
        let read = stream.read_exact(&mut self.addr).await?;
        if read != 16 {
            bail!("could not read 16 bytes of address for netaddrshort")
        }
        self.port = stream.read_u16().await?;
        Ok(())
    }

    fn serialize(&'_ self, stream: &mut impl BufMut) {
        stream.put_u64_le(self.services);
        stream.put(self.addr.as_slice());
        stream.put_u16(self.port);
    }
}
