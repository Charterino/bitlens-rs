use super::{
    inv::InventoryVector,
    packetpayload::PacketPayload,
    vec::Vec,
};

pub type GetData<'a> = Option<Vec<'a, InventoryVector>>;

pub const GETDATA_COMMAND: [u8; 12] = *b"getdata\0\0\0\0\0";

impl<'a, 'b> PacketPayload<'a, 'b> for GetData<'a> {
    fn command(&self) -> &'static [u8; 12] {
        return &GETDATA_COMMAND;
    }
}
