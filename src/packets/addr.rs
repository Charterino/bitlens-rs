use super::{netaddr::NetAddr, packetpayload::PacketPayload, vec::Vec};

pub type Addr<'a> = Option<Vec<'a, NetAddr>>;

pub const ADDR_COMMAND: [u8; 12] = *b"addr\0\0\0\0\0\0\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for Addr<'a> {
    fn command(&self) -> &'static [u8; 12] {
        return &ADDR_COMMAND;
    }
}
