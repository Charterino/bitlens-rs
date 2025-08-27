use hex::ToHex;
use serde::{Serializer, ser::SerializeMap};

pub fn serialize_spends<S>(spends: &[(u32, [u8; 32])], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut m = serializer.serialize_map(Some(spends.len()))?;
    for spend in spends {
        let mut reversed = spend.1;
        reversed.reverse();
        let hex_string = reversed.encode_hex::<String>();
        m.serialize_entry(&spend.0, &hex_string)?;
    }

    m.end()
}
