use std::borrow::Cow;

use super::{netaddr::NetAddr, packetpayload::PacketPayload};

pub type Addr<'a> = Cow<'a, [Cow<'a, NetAddr<'a>>]>;

pub const ADDR_COMMAND: [u8; 12] = *b"addr\0\0\0\0\0\0\0\0";

impl<'a> PacketPayload for Addr<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &ADDR_COMMAND
    }
}
