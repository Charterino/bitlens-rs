use super::{netaddr::NetAddrV2, packetpayload::PacketPayload};

pub type AddrV2<'a> = Option<&'a [&'a NetAddrV2<'a>]>;

pub const ADDRV2_COMMAND: [u8; 12] = *b"addrv2\0\0\0\0\0\0";

impl<'a> PacketPayload<'a> for AddrV2<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &ADDRV2_COMMAND
    }
}
