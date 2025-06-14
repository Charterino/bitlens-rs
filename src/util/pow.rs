use primitive_types::U256;

use super::compact::{compact_from_u256, u256_from_compact};

// These consts are from src/kernel/chainparams.cpp
pub const POW_TARGET_TIMESPAN: u64 = 14 * 24 * 60 * 60; // target: 2 weeks between difficulty adjustment
pub const POW_TARGET_SPACING: u64 = 10 * 60; // target: 10 minutes between blocks
pub static POW_LIMIT: U256 = U256([
    // little endian
    0xffffffffffffffff,
    0xffffffffffffffff,
    0xffffffffffffffff,
    0x00000000ffffffff,
]); // 0x00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
pub const DIFFICULTY_ADJUSTMENT_INTERVAL: u64 = POW_TARGET_TIMESPAN / POW_TARGET_SPACING;

// These functions are from src/pow.cpp
pub fn calculate_next_work_required(
    last_timestamp: u32,
    first_block_time: u32,
    last_bits: u32,
) -> u32 {
    let mut actual_timestamp = (last_timestamp - first_block_time) as u64;
    if actual_timestamp < POW_TARGET_TIMESPAN / 4 {
        actual_timestamp = POW_TARGET_TIMESPAN / 4;
    } else if actual_timestamp > POW_TARGET_TIMESPAN * 4 {
        actual_timestamp = POW_TARGET_TIMESPAN * 4;
    }

    let mut difficulty = (u256_from_compact(last_bits) * actual_timestamp) / POW_TARGET_TIMESPAN;
    if difficulty > POW_LIMIT {
        difficulty = POW_LIMIT;
    }
    compact_from_u256(difficulty)
}
