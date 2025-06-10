use anyhow::{Result, bail};
use tokio::io::{AsyncRead, AsyncReadExt};

use super::{MAX_PACKET_SIZE, magic::Magic};

#[derive(Debug)]
pub struct PacketHeader {
    pub magic: Magic,
    pub command: [u8; 12],
    pub length: u32,
    pub checksum: u32,
}

pub async fn read_header(stream: &mut (impl AsyncRead + Unpin)) -> Result<PacketHeader> {
    let magic = AsyncReadExt::read_u32_le(stream).await?;
    let mut command = [0u8; 12];
    AsyncReadExt::read_exact(stream, &mut command).await?;
    let length = AsyncReadExt::read_u32_le(stream).await?;
    let mut checksum = [0u8; 4];
    AsyncReadExt::read_exact(stream, &mut checksum).await?;

    if length as usize > MAX_PACKET_SIZE {
        bail!("packet size too large")
    }

    Ok(PacketHeader {
        magic: magic,
        command: command,
        length: length,
        checksum: u32::from_le_bytes(checksum),
    })
}
