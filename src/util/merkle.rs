use std::hint::unreachable_unchecked;

use sha2::{Digest, Sha256};

pub struct MerkleTree(Vec<u8>);

impl MerkleTree {
    pub fn new() -> Self {
        MerkleTree(Vec::new())
    }

    pub fn with_capacity(cap: usize) -> Self {
        MerkleTree(Vec::with_capacity(cap * 32))
    }

    pub fn append_hash(&mut self, hash: &[u8; 32]) {
        self.0.extend_from_slice(hash);
    }

    pub fn len(&self) -> usize {
        self.0.len() / 32
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    // from bitcoin:src/consensus/merkle.cpp:46
    // returns (root hash, modified)
    pub fn into_root(mut self) -> ([u8; 32], bool) {
        let mut mutation = false;
        while self.len() > 1 {
            for i in 0..self.len() - 1 {
                if self.get_hash(i) == self.get_hash(i + 1) {
                    mutation = true;
                }
            }
            if self.len() & 1 == 1 {
                // repeat the last element
                self.0.extend_from_within((self.len() - 1) * 32..);
            }
            self.collapse();
            self.0.truncate(self.0.len() / 2);
        }

        if self.0.is_empty() {
            ([0u8; 32], false)
        } else {
            (self.get_hash(0), mutation)
        }
    }

    fn collapse(&mut self) {
        if self.len() & 1 == 1 {
            unsafe {
                unreachable_unchecked();
            }
        }
        let blocks = self.len() / 2;
        for i in 0..blocks {
            let block = self.get_block(i);
            let shad: [u8; 32] = Sha256::digest(Sha256::digest(block)).into();
            self.set_hash(i, shad);
        }
    }

    #[inline(always)]
    fn get_block(&self, block_idx: usize) -> [u8; 64] {
        let block_start = block_idx * 2 * 32;
        let mut block: [u8; 64] = [0u8; 64];
        block.copy_from_slice(&self.0[block_start..block_start + 64]);
        block
    }

    #[inline(always)]
    fn get_hash(&self, hash_idx: usize) -> [u8; 32] {
        let hash_start = hash_idx * 32;
        let mut hash: [u8; 32] = [0u8; 32];
        hash.copy_from_slice(&self.0[hash_start..hash_start + 32]);
        hash
    }

    #[inline(always)]
    fn set_hash(&mut self, index: usize, hash: [u8; 32]) {
        self.0[index * 32..(index + 1) * 32].copy_from_slice(&hash);
    }
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self::new()
    }
}
