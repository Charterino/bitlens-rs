use primitive_types::U256;
use serde::Serialize;

use crate::packets::blockheader::BlockHeaderOwned;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeaderWithNumber {
    #[serde(flatten)]
    pub header: BlockHeaderOwned,
    pub number: u64,
    pub fetched_full: bool,
    pub total_work: U256,
    pub coinbase_ascii: Option<String>,
}
