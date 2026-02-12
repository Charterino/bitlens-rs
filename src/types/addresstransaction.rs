use serde::Serialize;

use crate::tx::address::Address;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressTransaction {
    #[serde(serialize_with = "crate::util::serialize_as_hex::serialize_hash_as_hex_reversed")]
    pub transaction_hash: [u8; 32],
    #[serde(serialize_with = "crate::util::serialize_as_hex::serialize_hash_as_hex_reversed")]
    pub block_hash: [u8; 32],
    pub block_number: u64,
    pub timestamp: u32,
    pub value: i64,

    // If the tx has the address in the txouts and it only has 1 distinct input address, put it here
    // If the tx has the address in the txins and it only has 1 distinct input address, put it here
    pub single_other_address: Option<Address>,

    pub distinct_other_addresses: usize,
}
