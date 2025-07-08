#[derive(Debug, Default, Clone)]
pub struct BlockMetrics {
    pub fees_total: u64,
    pub volume: u64,
    pub txs_count: u64,
    pub average_fee_rate: f64,
    pub lowest_fee_rate: f64,
    pub highest_fee_rate: f64,
    pub median_fee_rate: f64,
}
