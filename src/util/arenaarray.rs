use std::{marker::PhantomData, ptr};

#[derive(Debug, Clone, Copy)]
pub struct ArenaArray<'arena, T> {
    ptr: *mut T,
    len: usize,
    cap: usize,
    b: PhantomData<&'arena ()>,
}

impl<'arena, T> ArenaArray<'arena, T> {
    #[inline(always)]
    pub fn from_raw_parts(ptr: *mut T, cap: usize) -> Self {
        Self {
            ptr,
            len: 0,
            cap,
            b: PhantomData,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline(always)]
    pub fn cap(&self) -> usize {
        self.cap
    }

    // Will panic if there's no space left.
    #[inline(always)]
    pub fn push(&mut self, item: T) {
        if self.len == self.cap {
            panic!("arena array is full")
        }
        unsafe {
            ptr::write(self.ptr.add(self.len), item);
        }
        self.len += 1
    }

    #[inline(always)]
    pub fn into_arena_array(self) -> &'arena [T] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}
