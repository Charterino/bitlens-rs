use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct AddressTransaction {
    #[serde(serialize_with = "crate::util::serialize_as_hex::serialize_hash_as_hex_reversed")]
    pub transaction_hash: [u8; 32],
    #[serde(serialize_with = "crate::util::serialize_as_hex::serialize_hash_as_hex_reversed")]
    pub block_hash: [u8; 32],
    pub block_number: u64,
    pub timestamp: u32,
    pub value: i64,
}
