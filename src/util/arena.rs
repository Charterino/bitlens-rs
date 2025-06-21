use anyhow::{Result, bail};
use core::slice;
use std::{
    alloc::{Layout, alloc, dealloc},
    mem::transmute,
    ptr,
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
        Self {
            data: unsafe { alloc(Layout::from_size_align_unchecked(capacity, 1)) },
            capacity,
            consumed: AtomicUsize::new(0),
        }
    }

    #[inline(always)]
    pub fn try_alloc<T: Sized>(&self, value: T) -> Result<&mut T> {
        let t_size = size_of::<T>();
        let mut currently_consumed = self.consumed.load(Ordering::Relaxed);
        let cap = self.capacity;
        while currently_consumed + t_size <= cap {
            match self.consumed.compare_exchange_weak(
                currently_consumed,
                currently_consumed + t_size,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => unsafe {
                    let location = self.data.add(currently_consumed);
                    let casted: *mut T = transmute(location);
                    casted.write(value);
                    return Ok(&mut *casted);
                },
                Err(new_currently_consumed) => currently_consumed = new_currently_consumed,
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
        let size = size_t * count;
        let mut currently_consumed = self.consumed.load(Ordering::Relaxed);
        let cap = self.capacity;
        let mut padding = if size != 0 {
            size - (self.data as usize + currently_consumed) % size
        } else {
            0
        };
        while currently_consumed + size <= cap {
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
                    padding = if size != 0 {
                        size - (self.data as usize + currently_consumed) % size
                    } else {
                        0
                    };
                }
            }
        }
        bail!("arena: out of memory")
    }

    #[inline(always)]
    pub fn try_alloc_array_copy<T: Sized + Copy>(&self, value: &[T]) -> Result<&mut [T]> {
        let size = size_of::<T>() * value.len();
        let mut currently_consumed = self.consumed.load(Ordering::Relaxed);
        let cap = self.capacity;
        let mut padding = if size != 0 {
            size - (self.data as usize + currently_consumed) % size
        } else {
            0
        };
        while currently_consumed + size <= cap {
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
                    padding = if size != 0 {
                        size - (self.data as usize + currently_consumed) % size
                    } else {
                        0
                    };
                }
            }
        }
        bail!("arena: out of memory")
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
