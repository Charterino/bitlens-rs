use primitive_types::U256;

use crate::packets::blockheader::BlockHeader;

#[derive(Clone)]
pub struct BlockHeaderWithNumber<'a> {
    pub header: BlockHeader<'a>,
    pub number: u64,
    pub fetched_full: bool,
    pub total_work: U256,
}
