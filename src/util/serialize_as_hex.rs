use hex::ToHex;
use serde::{Serialize, Serializer};

pub fn serialize_as_hex<S, T>(bytes: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: AsRef<[u8]> + ?Sized,
{
    let hex_string = bytes.as_ref().encode_hex::<String>();
    hex_string.serialize(serializer)
}

pub fn serialize_hash_as_hex_reversed<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut reversed = *bytes;
    reversed.reverse();
    let hex_string = reversed.encode_hex::<String>();
    hex_string.serialize(serializer)
}

pub fn serialize_option_hash_as_hex_reversed<S>(
    bytes: &Option<[u8; 32]>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match bytes {
        Some(v) => {
            let mut reversed = *v;
            reversed.reverse();
            let hex_string = reversed.encode_hex::<String>();
            hex_string.serialize(serializer)
        }
        None => None::<[u8; 32]>.serialize(serializer),
    }
}
