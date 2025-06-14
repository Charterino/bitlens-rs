use crate::packets::blockheader::BlockHeader;

pub struct BlockHeaderWithNumber<'a> {
    pub header: BlockHeader<'a>,
    pub number: u64,
    pub fetched_full: bool,
}
