use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Address {
    P2PKFull { value: String },
    P2PKShort { value: String },
    P2PKH { value: String },
    P2SH { value: String },
    P2WSH { value: String },
    P2WPKH { value: String },
    Taproot { value: String },
    OpReturn,
    Unknown,
    Coinbase,
}
