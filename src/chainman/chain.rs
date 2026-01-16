use std::collections::{HashMap, HashSet};

use crate::{
    chainman::HeaderApplicationResult,
    packets::blockheader::BlockHeaderRef,
    types::blockheaderwithnumber::BlockHeaderWithNumber,
    util::{
        genesis::GENESIS_HEADER,
        pow::{DIFFICULTY_ADJUSTMENT_INTERVAL, calculate_next_work_required},
    },
};

pub struct Chain {
    pub top_header: BlockHeaderWithNumber,
    pub known_headers: HashMap<[u8; 32], BlockHeaderWithNumber>,
    pub most_work_chain: HashSet<[u8; 32]>,
}

impl Default for Chain {
    fn default() -> Self {
        let mut known = HashMap::with_capacity(1024 * 1024);
        let mut most_work_chain = HashSet::with_capacity(1024 * 1024);
        let genesis = BlockHeaderWithNumber {
            header: *GENESIS_HEADER,
            number: 0,
            fetched_full: false,
            total_work: BlockHeaderRef::Owned(&GENESIS_HEADER).get_work(),
        };
        known.insert(GENESIS_HEADER.hash, genesis.clone());
        most_work_chain.insert(GENESIS_HEADER.hash);
        Self {
            known_headers: known,
            top_header: genesis,
            most_work_chain,
        }
    }
}

impl Chain {
    pub fn get_ancestor<'a>(
        &'a self,
        header: &'a BlockHeaderWithNumber,
        height: u64,
    ) -> Option<&'a BlockHeaderWithNumber> {
        let mut top = header;
        while top.number > height {
            let next = self.known_headers.get(top.header.parent.as_slice());
            match next {
                Some(next) => {
                    top = next;
                }
                None => return None,
            }
        }

        Some(top)
    }

    pub fn get_next_bits_required(&self, header: &BlockHeaderWithNumber) -> u32 {
        if (header.number + 1) % DIFFICULTY_ADJUSTMENT_INTERVAL != 0 {
            return header.header.bits;
        }

        let first_height = header.number - (DIFFICULTY_ADJUSTMENT_INTERVAL - 1);
        let first_header = match self.get_ancestor(header, first_height) {
            Some(h) => h,
            None => {
                panic!("get_next_bits_required called on a header without proper history")
            }
        };

        calculate_next_work_required(
            header.header.timestamp,
            first_header.header.timestamp,
            header.header.bits,
        )
    }

    pub fn update_most_work_chain(&mut self, result: &HeaderApplicationResult) {
        for hash in &result.removed_blocks {
            self.most_work_chain.remove(hash);
        }
        for hash in &result.added_blocks {
            self.most_work_chain.insert(*hash);
        }
    }
}
