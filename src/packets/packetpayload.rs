use anyhow::Result;
use bumpalo::boxed::Box;
use bytes::BufMut;
use tokio::io::BufReader;
use tokio::net::tcp::ReadHalf;

use super::addr::Addr;
use super::addrv2::AddrV2;
use super::block::Block;
use super::getaddr::GetAddr;
use super::ping::Ping;
use super::pong::Pong;
use super::sendaddrv2::SendAddrV2;
use super::sendheaders::SendHeaders;
use super::tx::Tx;
use super::varint::VarInt;
use super::verack::VerAck;
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

    fn serialize(&self, stream: &mut impl BufMut);
}

pub enum PacketPayloadType<'a> {
    Version(Box<'a, Version<'a>>),
    Addr(Box<'a, Addr<'a>>),
    AddrV2(Box<'a, AddrV2<'a>>),
    GetAddr(Box<'a, GetAddr>),
    Ping(Box<'a, Ping>),
    Pong(Box<'a, Pong>),
    VerAck(Box<'a, VerAck>),
    SendHeaders(Box<'a, SendHeaders>),
    SendAddrV2(Box<'a, SendAddrV2>),
    Tx(Box<'a, Tx<'a>>),
    Block(Box<'a, Block<'a>>),
}

impl<'a, 'b, T: Serializable<'a, 'b> + Default + Clone> Serializable<'a, 'b>
    for Option<super::vec::Vec<'a, T>>
{
    async fn deserialize(
        &mut self,
        allocator: &'a bumpalo::Bump<1>,
        stream: &mut BufReader<ReadHalf<'b>>,
    ) -> Result<()> {
        let mut len = 0 as VarInt;
        len.deserialize(allocator, stream).await?;
        if len == 0 {
            *self = None;
            return Ok(());
        }

        let mut result = bumpalo::vec![in allocator; T::default(); len as usize];
        for i in 0..len as usize {
            result
                .get_mut(i)
                .unwrap()
                .deserialize(allocator, stream)
                .await?;
        }

        *self = Some(super::vec::Vec::Bumpalod(result));

        Ok(())
    }

    fn serialize(&self, stream: &mut impl BufMut) {
        match self {
            Some(array) => {
                let len = array.len() as VarInt;
                len.serialize(stream);
                for i in 0..len as usize {
                    array.get(i).unwrap().serialize(stream);
                }
            }
            None => stream.put_u8(0),
        }
    }
}
