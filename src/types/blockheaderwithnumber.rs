use primitive_types::U256;

use crate::packets::blockheader::BlockHeaderOwned;

#[derive(Clone)]
pub struct BlockHeaderWithNumber {
    pub header: BlockHeaderOwned,
    pub number: u64,
    pub fetched_full: bool,
    pub total_work: U256,
}
