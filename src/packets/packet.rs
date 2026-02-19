use super::block::BlockBorrowed;
use super::packetheader::{PacketHeader, read_header};
use crate::packets::arenapool::ArenaPool;
use crate::packets::packetpayload::{ReceivedPayload, deserialize_payload};
use crate::util::arena::Arena;
use crate::util::env::get_env;
use crate::util::speedtracker::read_payload;
use anyhow::Result;
use deadpool::unmanaged::Object;
use deadpool::unmanaged::Pool;
use ouroboros::self_referencing;
use slog_scope::debug;
use std::sync::LazyLock;
use tokio::io::BufReader;
use tokio::net::tcp::OwnedReadHalf;

pub const MAX_PACKET_SIZE: usize = 4 * 1024 * 1024;
pub const SMALL_ARENA_SIZE: usize = 8 * 1024;

pub static SERIALIZE_POOL: LazyLock<Pool<Vec<u8>>> = LazyLock::new(|| {
    let mut pool = Vec::with_capacity(32);
    for _ in 0..32 {
        pool.push(Vec::with_capacity(MAX_PACKET_SIZE));
    }
    Pool::from(pool)
});

// So we start off with 64 4mb arenas. During block download we can have as many as 1k of them. After that, it's gonna go back to 64.
// See deserializearenas.png for a drawing explaining this process.
// We also have a similar setup for 4kb arenas but in higher quantities.
const DEFAULT_INITIAL_LARGE_DESERIALIZE_ARENAS_COUNT: usize = 64;
const DEFAULT_LARGE_DESERIALIZE_ARENAS_COUNT_BLOCKSYNC: usize = 512;
pub static INITIAL_LARGE_DESERIALIZE_ARENAS_COUNT: LazyLock<usize> = LazyLock::new(|| {
    get_env(
        "INITIAL_LARGE_DESERIALIZE_ARENAS_COUNT",
        DEFAULT_INITIAL_LARGE_DESERIALIZE_ARENAS_COUNT,
    )
});
pub static LARGE_DESERIALIZE_ARENAS_COUNT_BLOCKSYNC: LazyLock<usize> = LazyLock::new(|| {
    get_env(
        "LARGE_DESERIALIZE_ARENAS_COUNT_BLOCKSYNC",
        DEFAULT_LARGE_DESERIALIZE_ARENAS_COUNT_BLOCKSYNC,
    )
});

const DEFAULT_INITIAL_SMALL_DESERIALIZE_ARENAS_COUNT: usize = 256;
const DEFAULT_SMALL_DESERIALIZE_ARENAS_COUNT_BLOCKSYNC: usize = 1024;
pub static INITIAL_SMALL_DESERIALIZE_ARENAS_COUNT: LazyLock<usize> = LazyLock::new(|| {
    get_env(
        "INITIAL_SMALL_DESERIALIZE_ARENAS_COUNT",
        DEFAULT_INITIAL_SMALL_DESERIALIZE_ARENAS_COUNT,
    )
});
pub static SMALL_DESERIALIZE_ARENAS_COUNT_BLOCKSYNC: LazyLock<usize> = LazyLock::new(|| {
    get_env(
        "SMALL_DESERIALIZE_ARENAS_COUNT_BLOCKSYNC",
        DEFAULT_SMALL_DESERIALIZE_ARENAS_COUNT_BLOCKSYNC,
    )
});

pub static DESERIALIZE_POOL_LARGE: LazyLock<ArenaPool> =
    LazyLock::new(|| ArenaPool::new(MAX_PACKET_SIZE, *INITIAL_LARGE_DESERIALIZE_ARENAS_COUNT));

pub static DESERIALIZE_POOL_SMALL: LazyLock<ArenaPool> =
    LazyLock::new(|| ArenaPool::new(SMALL_ARENA_SIZE, *INITIAL_SMALL_DESERIALIZE_ARENAS_COUNT));

pub struct Packet {
    pub header: PacketHeader,
    pub payload: PayloadWithAllocator,
}

#[self_referencing]
pub struct PayloadWithAllocator {
    pub allocator_with_buffer: AllocatorWithBuffer,
    #[borrows(allocator_with_buffer)]
    #[covariant]
    pub payload: Option<ReceivedPayload<'this>>,
}

impl PayloadWithAllocator {
    pub fn with_block<F, R>(&self, user: F) -> R
    where
        F: FnOnce(&BlockBorrowed) -> R,
    {
        let payload = self.borrow_payload();
        if let ReceivedPayload::Block(b) = payload.as_ref().expect("the payload to not be empty") {
            user(b)
        } else {
            unreachable!()
        }
    }

    pub async fn with_block_async<F>(&self, user: F)
    where
        F: AsyncFnOnce(&BlockBorrowed) + Send,
    {
        let payload = self.borrow_payload();
        if let ReceivedPayload::Block(b) = payload.as_ref().expect("the payload to not be empty") {
            user(b).await
        } else {
            unreachable!()
        }
    }
}

#[self_referencing]
pub struct AllocatorWithBuffer {
    pub allocator: Object<Arena>,
    #[borrows(allocator)]
    pub buffer: &'this mut [u8],
}

pub async fn read_packet(stream: &mut BufReader<OwnedReadHalf>) -> Result<Packet> {
    let header = read_header(stream).await?;

    let allocator = if header.length as usize > SMALL_ARENA_SIZE / 2 {
        DESERIALIZE_POOL_LARGE.get_arena().await
    } else {
        DESERIALIZE_POOL_SMALL.get_arena().await
    };

    let mut allocator_with_buffer: AllocatorWithBuffer = AllocatorWithBufferBuilder {
        allocator,
        buffer_builder: |allocator: &Object<Arena>| {
            allocator
                .try_alloc_array_fill_copy(header.length as usize, 0u8)
                .unwrap()
        },
    }
    .build();

    allocator_with_buffer
        .with_buffer_mut(|buffer: &mut &mut [u8]| read_payload(stream, buffer))
        .await?;

    let payload_with_allocator = PayloadWithAllocatorBuilder {
        allocator_with_buffer,
        payload_builder: |awb: &AllocatorWithBuffer| match deserialize_payload(
            awb,
            header.checksum,
            header.command,
        ) {
            Ok(a) => a,
            Err(e) => {
                debug!("failed to deserialize payload"; "e" => e.to_string());
                // if we cant deserialize the payload, dont return an error but "ignore it".
                None
            }
        },
    }
    .build();

    Ok(Packet {
        header,
        payload: payload_with_allocator,
    })
}
