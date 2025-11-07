use serde::{Serialize, Serializer};

pub fn serialize_try_string<S>(ua: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match ua {
        Some(data) => {
            let str = String::from_utf8_lossy(data);
            str.serialize(serializer)
        }
        None => "".serialize(serializer),
    }
}
