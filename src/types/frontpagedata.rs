use serde::{Deserialize, Serialize};

use super::stats::Stats;

pub struct FrontPageDataWithSerialized {
    pub data: FrontPageData,
    pub serialized: String,
}

impl Default for FrontPageDataWithSerialized {
    fn default() -> Self {
        let data = FrontPageData::default();
        let serialized = serde_json::to_string(&data).unwrap();
        Self { data, serialized }
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontPageData {
    pub stats: Stats,
    pub latest_blocks: Vec<ShortBlock>,
    pub latest_txs: Vec<ShortTx>,
    pub sync_stats: Option<SyncStats>,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStats {
    pub total_blocks: u64,
    pub synced_blocks: u64,
    pub approx_remaining_seconds: f64,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortBlock {
    pub number: u64,
    pub hash: String,
    pub tx_count: u64,
    pub reward_btc: f64,
    pub btc_price: f64,
    pub timestamp: u32,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortTx {
    pub hash: String,
    pub value: f64,
    pub size_wus: u32,
    pub timestamp: u32,
    pub block_number: u64,
    pub block_hash: String,
    pub fee_sats: u64,
    pub btc_price: f64,
}
