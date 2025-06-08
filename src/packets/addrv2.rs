use super::{netaddr::NetAddrV2, packetpayload::PacketPayload, vec::Vec};

pub type AddrV2<'a> = Option<Vec<'a, NetAddrV2<'a>>>;

pub const ADDRV2_COMMAND: [u8; 12] = *b"addrv2\0\0\0\0\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for AddrV2<'a> {
    fn command(&self) -> &'static [u8; 12] {
        return &ADDRV2_COMMAND;
    }
}
