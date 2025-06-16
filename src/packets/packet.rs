use std::sync::LazyLock;

use crate::packets::packetpayload::{deserialize_payload, read_payload};

use super::packetheader::{PacketHeader, read_header};
use super::packetpayload::PacketPayloadType;
use anyhow::Result;
use bumpalo::{Bump, vec};
use deadpool::unmanaged::Object;
use deadpool::unmanaged::Pool;
use ouroboros::self_referencing;
use tokio::io::BufReader;
use tokio::net::tcp::OwnedReadHalf;

pub const MAX_PACKET_SIZE: usize = 4 * 1024 * 1024;

pub static SERIALIZE_POOL: LazyLock<Pool<Vec<u8>>> = LazyLock::new(|| {
    let mut pool = Vec::with_capacity(32);
    for _ in 0..32 {
        pool.push(Vec::with_capacity(MAX_PACKET_SIZE));
    }
    Pool::from(pool)
});

static DESERIALIZE_POOL: LazyLock<Pool<Bump<1>>> = LazyLock::new(|| {
    let mut pool = Vec::with_capacity(1024);
    for _ in 0..1024 {
        let b = Bump::with_capacity(MAX_PACKET_SIZE);
        b.set_allocation_limit(Some(MAX_PACKET_SIZE));
        pool.push(b);
    }
    Pool::from(pool)
});

pub struct Packet {
    pub header: PacketHeader,
    pub payload: PayloadWithAllocator,
}

#[self_referencing]
pub struct PayloadWithAllocator {
    pub allocator_with_buffer: AllocatorWithBuffer,
    #[borrows(allocator_with_buffer)]
    #[not_covariant]
    pub payload: Option<PacketPayloadType<'this>>,
}

#[self_referencing]
pub struct AllocatorWithBuffer {
    pub allocator: Object<Bump>,
    #[borrows(allocator)]
    #[covariant]
    pub buffer: bumpalo::collections::Vec<'this, u8>,
}

unsafe impl Send for AllocatorWithBuffer {}

pub async fn read_packet(stream: &mut BufReader<OwnedReadHalf>) -> Result<Packet> {
    let header = read_header(stream).await?;

    let mut allocator = DESERIALIZE_POOL.get().await.unwrap();
    allocator.reset();

    let mut allocator_with_buffer: AllocatorWithBuffer = AllocatorWithBufferBuilder {
        allocator,
        buffer_builder: |allocator: &Object<Bump>| vec![in allocator; 0; header.length as usize],
    }
    .build();

    allocator_with_buffer
        .with_buffer_mut(|buffer: &mut bumpalo::collections::Vec<u8>| read_payload(stream, buffer))
        .await?;

    let payload_with_allocator = PayloadWithAllocatorTryBuilder {
        allocator_with_buffer,
        payload_builder: |awb: &AllocatorWithBuffer| {
            deserialize_payload(awb, header.checksum, header.command)
        },
    }
    .try_build()?;

    Ok(Packet {
        header,
        payload: payload_with_allocator,
    })
}
