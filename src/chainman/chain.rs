use std::collections::HashMap;

use crate::{
    packets::deepclone::DeepClone,
    types::blockheaderwithnumber::BlockHeaderWithNumber,
    util::{
        genesis::GENESIS_HEADER,
        pow::{DIFFICULTY_ADJUSTMENT_INTERVAL, calculate_next_work_required},
    },
};

pub struct Chain<'a> {
    pub top_header: BlockHeaderWithNumber<'a>,
    pub known_headers: HashMap<[u8; 32], BlockHeaderWithNumber<'a>>,
}

impl Default for Chain<'_> {
    fn default() -> Self {
        let mut known = HashMap::new();
        let genesis = BlockHeaderWithNumber {
            header: GENESIS_HEADER.deep_clone(),
            number: 0,
            fetched_full: false,
            total_work: GENESIS_HEADER.get_work(),
        };
        known.insert(GENESIS_HEADER.hash, genesis.clone());
        Self {
            known_headers: known,
            top_header: genesis,
        }
    }
}

impl<'a> Chain<'a> {
    pub fn get_ancestor(
        &'a self,
        header: &'a BlockHeaderWithNumber<'a>,
        height: u64,
    ) -> Option<&'a BlockHeaderWithNumber<'a>> {
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
}
