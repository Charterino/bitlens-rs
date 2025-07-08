use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub average_median_fees: HashMap<u32, f64>,
    pub transactions: HashMap<u32, u64>,
    pub volume: HashMap<u32, f64>,
}
