use std::borrow::Cow;

use super::{inv::InventoryVector, packetpayload::PacketPayload};

pub type GetData<'a> = Cow<'a, [Cow<'a, InventoryVector<'a>>]>;

pub const GETDATA_COMMAND: [u8; 12] = *b"getdata\0\0\0\0\0";

impl<'a> PacketPayload for GetData<'a> {
    fn command(&self) -> &'static [u8; 12] {
        &GETDATA_COMMAND
    }
}
