use anyhow::Result;
use bumpalo::boxed::Box;
use bytes::BufMut;
use tokio::io::BufReader;
use tokio::net::tcp::ReadHalf;

use super::version::Version;

pub trait PacketPayload<'bump, 'stream>: Default + Serializable<'bump, 'stream> {
    fn command(&self) -> &'static [u8; 12];
}

pub trait Serializable<'bump, 'stream> {
    async fn deserialize(
        &mut self,
        allocator: &'bump bumpalo::Bump<1>,
        stream: &mut BufReader<ReadHalf<'stream>>,
    ) -> Result<()>; // Not an Option<Error> to allow for ? shorthand

    fn serialize(&'bump self, stream: &mut impl BufMut);
}

pub enum PacketPayloadType<'a> {
    Version(Box<'a, Version<'a>>),
}
