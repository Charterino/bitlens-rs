use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
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

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Address::P2PKFull { value } => f.write_str(value),
            Address::P2PKShort { value } => f.write_str(value),
            Address::P2PKH { value } => f.write_str(value),
            Address::P2SH { value } => f.write_str(value),
            Address::P2WSH { value } => f.write_str(value),
            Address::P2WPKH { value } => f.write_str(value),
            Address::Taproot { value } => f.write_str(value),
            Address::OpReturn => f.write_str("OP_RETURN"),
            Address::Unknown => f.write_str("Unknown"),
            Address::Coinbase => f.write_str("Coinbase"),
        }
    }
}
