use bytes::BufMut;
use tokio::io::AsyncReadExt;

use super::{
    packetpayload::{Serializable, Stream},
    varint::VarInt,
    vec::Vec,
};

// not necessarily valid UTF-8
pub type VarStr<'a> = Option<Vec<'a, u8>>;

impl<'a, 'b> Serializable<'a, 'b> for VarStr<'a> {
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut impl Stream,
    ) -> anyhow::Result<()> {
        let mut length = 0 as VarInt;
        length.deserialize(allocator, stream).await?;
        if length == 0 {
            *self = None;
            return Ok(());
        }
        let mut storage = bumpalo::vec![in &allocator; 0u8; length as usize];
        // storage.as_mut_slice will return an empty slice because storage's length is 0
        stream.read_exact(storage.as_mut_slice()).await?;
        *self = Some(Vec::Bumpalod(storage));

        Ok(())
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        match &self {
            Some(ua) => {
                let len = ua.len() as VarInt;
                len.serialize(stream);
                stream.put(ua.as_slice());
            }
            None => {
                stream.put_u8(0);
            }
        }
    }
}
