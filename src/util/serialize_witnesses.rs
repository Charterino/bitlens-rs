use hex::ToHex;
use serde::{Serializer, ser::SerializeSeq};

use crate::packets::varstr::deserialize_array_of_varsrs_iter;

pub fn serialize_witnesses<S>(
    witnesses: &Option<Vec<Vec<u8>>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let witnesses = match witnesses {
        Some(w) => w,
        None => return serializer.serialize_seq(Some(0))?.end(),
    };
    let mut seq = serializer.serialize_seq(Some(witnesses.len()))?;
    for witness in witnesses {
        seq.serialize_element(&ChunkedHexData(witness))?;
    }

    seq.end()
}

struct ChunkedHexData<'a>(&'a Vec<u8>);

impl serde::Serialize for ChunkedHexData<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match deserialize_array_of_varsrs_iter(self.0) {
            Ok(components) => {
                let mut seq = serializer.serialize_seq(None)?;
                for component in components {
                    let hex_str = component.encode_hex::<String>();
                    seq.serialize_element(&hex_str)?;
                }
                seq.end()
            }
            Err(_) => serializer.serialize_seq(Some(0))?.end(),
        }
    }
}
