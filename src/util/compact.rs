
use primitive_types::U256;

pub fn u256_from_compact(compact: u32) -> U256 {
    let size = compact >> 24;
    let word = compact & 0x007FFFFF;
    let mut ret = U256::from(word);
    if size <= 3 {
        ret = ret >> (8 * (3 - size));
    } else {
        ret = ret << (8 * (size - 3));
    }
    return ret;
}

// https://github.com/bitcoin/bitcoin/blob/master/src/arith_uint256.cpp#L195
pub fn compact_from_u256(mut value: U256) -> u32 {
    let mut compact: u32;
    let mut size = (value.bits() + 7) / 8;
    if size <= 3 {
        compact = value.as_u32() << (8 * (3 - size));
    } else {
        value = value >> (8 * (size - 3));
        compact = value.as_u32();
    }

    if (compact & 0x00800000) == 0x00800000 {
        compact = compact >> 8;
        size += 1;
    }

    compact = compact | ((size << 24) as u32);
    compact
}
