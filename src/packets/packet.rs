use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::packets::packetpayload::{deserialize_payload, read_payload};

use super::packetheader::{PacketHeader, read_header};
use super::packetpayload::PacketPayloadType;
use anyhow::Result;
use bumpalo::{Bump, vec};
use deadpool::unmanaged::Pool;
use deadpool::unmanaged::{Object, PoolConfig};
use ouroboros::self_referencing;
use rand::Rng;
use slog_scope::debug;
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

// So we start off with 128 4mb arenas. During block download we can have as many as 2k of them. After that, it's gonna go back to 128.
// See deserializearenas.png for a drawing explaining this process.
pub const DESERIALIZE_POOL_TIMEOUT: Duration = Duration::from_millis(100);
pub const INITIAL_DESERIALIZE_ARENA_COUNT: usize = 128;
pub const MAX_DESERIALIZE_ARENA_COUNT_DURING_BLOCKSYNC: usize = 2048;
pub static CURRENT_POOL_LIMIT: AtomicUsize = AtomicUsize::new(INITIAL_DESERIALIZE_ARENA_COUNT);
pub static CURRENT_POOL_SIZE: AtomicUsize = AtomicUsize::new(INITIAL_DESERIALIZE_ARENA_COUNT);
static DESERIALIZE_POOL: LazyLock<Pool<Bump<1>>> = LazyLock::new(|| {
    let mut config = PoolConfig::new(1024 * 1024);
    config.runtime = Some(deadpool::Runtime::Tokio1);
    config.timeout = Some(DESERIALIZE_POOL_TIMEOUT);

    let pool = Pool::from_config(&config);
    for _ in 0..INITIAL_DESERIALIZE_ARENA_COUNT {
        let b = Bump::with_capacity(MAX_PACKET_SIZE);
        b.set_allocation_limit(Some(MAX_PACKET_SIZE));
        pool.try_add(b).expect("to have added arena");
    }
    pool
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

    let mut allocator = None;
    while allocator.is_none() {
        match DESERIALIZE_POOL.get().await {
            Ok(p) => {
                // Should we drop this arena?
                let mut current_size = CURRENT_POOL_SIZE.load(Ordering::SeqCst);
                let limit = CURRENT_POOL_LIMIT.load(Ordering::SeqCst);
                if current_size <= limit {
                    allocator = Some(p);
                    break;
                }
                while current_size < limit {
                    match CURRENT_POOL_SIZE.compare_exchange_weak(
                        current_size,
                        current_size - 1,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    ) {
                        Ok(_) => {
                            // Dropping this arena
                            let _ = Object::take(p);
                            debug!("removed a deserialize arena"; "limit" => limit, "new_size" => current_size - 1);
                            break;
                        }
                        Err(new_current) => current_size = new_current,
                    }
                }
            }
            Err(_) => {
                // Can we add another arena?
                let mut current_size = CURRENT_POOL_SIZE.load(Ordering::SeqCst);
                let limit = CURRENT_POOL_LIMIT.load(Ordering::Relaxed);
                while current_size < limit {
                    match CURRENT_POOL_SIZE.compare_exchange_weak(
                        current_size,
                        current_size + 1,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    ) {
                        Ok(_) => {
                            // Adding new arena.
                            let b = Bump::with_capacity(MAX_PACKET_SIZE);
                            b.set_allocation_limit(Some(MAX_PACKET_SIZE));
                            DESERIALIZE_POOL
                                .try_add(b)
                                .expect("to have added a new arena");
                            debug!("added a new deserialize arena"; "limit" => limit, "new_size" => current_size + 1);
                            break;
                        }
                        Err(new_current) => current_size = new_current,
                    }
                }
                // If we can't add a new arena, the loop will continue waiting.
            }
        }
    }
    let mut allocator = allocator.unwrap();
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
        payload_builder: |awb: &AllocatorWithBuffer| match deserialize_payload(
            awb,
            header.checksum,
            header.command,
        ) {
            Ok(a) => Ok(a),
            Err(e) => {
                debug!("failed to deserialize payload"; "e" => e.to_string());
                Err(e)
            }
        },
    }
    .try_build()?;

    Ok(Packet {
        header,
        payload: payload_with_allocator,
    })
}
