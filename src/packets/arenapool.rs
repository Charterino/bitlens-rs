use std::{
    sync::{
        LazyLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use deadpool::unmanaged::{Object, Pool, PoolConfig};
use slog_scope::warn;

use crate::util::arena::Arena;

pub static SHOULD_LOG_INSUFFICIENT_ARENAS: LazyLock<bool> =
    LazyLock::new(|| match std::env::var("DISABLE_INSUFFICIENT_ARENAS_LOG") {
        Ok(v) => v.is_empty(),
        Err(_) => true,
    });

pub const DESERIALIZE_POOL_TIMEOUT: Duration = Duration::from_millis(100);

pub struct ArenaPool {
    pool: Pool<Arena>,
    arena_size: usize,
    current_size: AtomicUsize,
    current_limit: AtomicUsize,
}

impl ArenaPool {
    pub fn new(arena_size: usize, initial_size: usize) -> Self {
        let pool = {
            let mut config = PoolConfig::new(1024 * 1024);
            config.runtime = Some(deadpool::Runtime::Tokio1);
            config.timeout = Some(DESERIALIZE_POOL_TIMEOUT);

            let pool = Pool::from_config(&config);
            for _ in 0..initial_size {
                let a = Arena::new(arena_size);
                pool.try_add(a).expect("to have added arena");
            }
            pool
        };
        Self {
            pool,
            arena_size,
            current_limit: AtomicUsize::new(initial_size),
            current_size: AtomicUsize::new(initial_size),
        }
    }

    pub fn set_limit(&self, new_limit: usize) {
        self.current_limit.store(new_limit, Ordering::Relaxed);
    }

    pub async fn get_arena(&self) -> Object<Arena> {
        let mut allocator = None;
        while allocator.is_none() {
            match self.pool.get().await {
                Ok(p) => {
                    // Should we drop this arena?
                    let mut current_size = self.current_size.load(Ordering::SeqCst);
                    let limit = self.current_limit.load(Ordering::SeqCst);
                    if current_size <= limit {
                        allocator = Some(p);
                        break;
                    }
                    while current_size > limit {
                        match self.current_size.compare_exchange_weak(
                            current_size,
                            current_size - 1,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        ) {
                            Ok(_) => {
                                // Dropping this arena
                                let _ = Object::take(p);
                                break;
                            }
                            Err(new_current) => current_size = new_current,
                        }
                    }
                }
                Err(_) => {
                    // Can we add another arena?
                    let mut current_size = self.current_size.load(Ordering::SeqCst);
                    let limit = self.current_limit.load(Ordering::Relaxed);
                    while current_size < limit {
                        match self.current_size.compare_exchange_weak(
                            current_size,
                            current_size + 1,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        ) {
                            Ok(_) => {
                                // Adding new arena.
                                let a = Arena::new(self.arena_size);
                                self.pool.try_add(a).expect("to have added a new arena");
                                break;
                            }
                            Err(new_current) => current_size = new_current,
                        }
                    }
                    // If we can't add a new arena, the loop will continue waiting.
                    if current_size >= limit && *SHOULD_LOG_INSUFFICIENT_ARENAS {
                        warn!("insufficient arenas, considering adding more"; "size" => current_size, "limit" => limit);
                    }
                }
            }
        }
        let allocator = allocator.unwrap();
        allocator.reset();

        allocator
    }

    pub fn get_size(&self) -> usize {
        self.current_size.load(Ordering::Relaxed)
    }

    pub fn get_limit(&self) -> usize {
        self.current_limit.load(Ordering::Relaxed)
    }
}
