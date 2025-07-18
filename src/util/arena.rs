use anyhow::{Result, bail};
use core::slice;
use std::{
    alloc::{Layout, alloc, dealloc},
    mem::transmute,
    ptr::{self, write_bytes},
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct Arena {
    data: *mut u8,
    consumed: AtomicUsize,
    capacity: usize,
}

unsafe impl Send for Arena {}
unsafe impl Sync for Arena {}

impl Arena {
    pub fn new(capacity: usize) -> Self {
        let alloced = unsafe {
            let v = alloc(Layout::from_size_align_unchecked(capacity, 1));
            // For now this fills the entire buffer with 0s so I can easily track memory. Soon to be changed.
            write_bytes(v, 0, capacity);
            v
        };
        Self {
            data: alloced,
            capacity,
            consumed: AtomicUsize::new(0),
        }
    }

    pub fn used(&self) -> usize {
        self.consumed.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn try_alloc<T: Sized>(&self, value: T) -> Result<&mut T> {
        let alignment = align_of::<T>();
        let size_t = size_of::<T>();
        let mut currently_consumed = self.consumed.load(Ordering::Relaxed);
        let cap = self.capacity;
        let mut padding = if alignment != 0 {
            alignment - ((self.data as usize + currently_consumed) % alignment)
        } else {
            0
        };
        while currently_consumed + padding + size_t <= cap {
            match self.consumed.compare_exchange_weak(
                currently_consumed,
                currently_consumed + padding + size_t,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => unsafe {
                    let location = self.data.add(currently_consumed);
                    let casted: *mut T = transmute(location);
                    casted.write(value);
                    return Ok(&mut *casted);
                },
                Err(new_currently_consumed) => {
                    currently_consumed = new_currently_consumed;
                    padding = if alignment != 0 {
                        alignment - ((self.data as usize + currently_consumed) % alignment)
                    } else {
                        0
                    };
                }
            }
        }
        bail!("arena: out of memory")
    }

    #[inline(always)]
    pub fn try_alloc_array_fill_copy<T: Sized + Copy>(
        &self,
        count: usize,
        value: T,
    ) -> Result<&mut [T]> {
        let size_t = size_of::<T>();
        let alignment = align_of::<T>();
        let size = size_t * count;
        let mut currently_consumed = self.consumed.load(Ordering::Relaxed);
        let cap = self.capacity;
        let mut padding = if alignment != 0 {
            alignment - ((self.data as usize + currently_consumed) % alignment)
        } else {
            0
        };
        while currently_consumed + padding + size <= cap {
            match self.consumed.compare_exchange_weak(
                currently_consumed,
                currently_consumed + padding + size,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => unsafe {
                    let location = self.data.add(padding + currently_consumed);
                    let casted: *mut T = transmute(location);
                    for i in 0..count {
                        ptr::write(casted.add(i), value);
                    }
                    return Ok(slice::from_raw_parts_mut(casted, count));
                },
                Err(new_currently_consumed) => {
                    currently_consumed = new_currently_consumed;
                    padding = if alignment != 0 {
                        alignment - ((self.data as usize + currently_consumed) % alignment)
                    } else {
                        0
                    };
                }
            }
        }
        bail!(
            "arena: out of memory, cap: {} currently_consumed: {}, wanted: {}",
            cap,
            currently_consumed,
            size
        )
    }

    #[inline(always)]
    pub fn try_alloc_array_copy<T: Sized + Copy>(&self, value: &[T]) -> Result<&mut [T]> {
        let alignment = align_of::<T>();
        let size = size_of_val(value);
        let mut currently_consumed = self.consumed.load(Ordering::Relaxed);
        let cap = self.capacity;
        let mut padding = if alignment != 0 {
            alignment - ((self.data as usize + currently_consumed) % alignment)
        } else {
            0
        };
        while currently_consumed + padding + size <= cap {
            match self.consumed.compare_exchange_weak(
                currently_consumed,
                currently_consumed + padding + size,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => unsafe {
                    let location = self.data.add(padding + currently_consumed);
                    let casted: *mut T = transmute(location);
                    core::ptr::copy_nonoverlapping(value.as_ptr(), casted, value.len());
                    return Ok(slice::from_raw_parts_mut(casted, value.len()));
                },
                Err(new_currently_consumed) => {
                    currently_consumed = new_currently_consumed;
                    padding = if alignment != 0 {
                        alignment - ((self.data as usize + currently_consumed) % alignment)
                    } else {
                        0
                    };
                }
            }
        }
        bail!("arena: out of memory")
    }

    pub fn reset(&self) {
        self.consumed.store(0, Ordering::SeqCst);
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        unsafe {
            dealloc(
                self.data,
                Layout::from_size_align_unchecked(self.capacity, 1),
            );
        }
    }
}
