use crate::packets::{
    tx::{TxBorrowed, TxOwned, TxRef},
    varint::{VarInt, length_varint},
};

pub fn calculate_tx_size_wus(tx: TxRef) -> u32 {
    match tx {
        TxRef::Borrowed(tx_borrowed) => calculate_tx_size_wus_borrowed(tx_borrowed),
        TxRef::Owned(tx_owned) => calculate_tx_size_wus_owned(tx_owned),
    }
}

fn calculate_tx_size_wus_owned(tx: &TxOwned) -> u32 {
    let mut result = 0;
    // First we count the non-segwit data at 4 WUs per bytes
    // Version is 4 bytes so 16 WUs
    result += 16;
    // Locktime is also 4 bytes so another 16 WUs
    result += 16;

    // Depending on how many txins there are, the varint will take a diff amount of space: 4 * bytes
    result += 4 * length_varint(tx.txins.len() as VarInt);
    // Now for every txin:
    for txin in tx.txins.iter() {
        let txin_size = 32 + // length of previous tx hash: 32 bytes
			4 + // length of previous tx output index: 4 bytes
			4 + // length of sequence: 4 bytes
			length_varint(txin.sig_script.len() as VarInt) + // length of varint length of sigScript
			txin.sig_script.len(); // length of sigscript
        // at 4 WUs per byte:
        result += txin_size * 4
    }
    // Depending on how many txouts there are, the varint will take a diff amount of space: 4 * bytes
    result += 4 * length_varint(tx.txouts.len() as VarInt);
    // Now for every txout:
    for txout in tx.txouts.iter() {
        let txout_size = 8 + // size of value
			length_varint(txout.script.len() as VarInt) + // length of varint length of scriptPubKey
			txout.script.len(); // length of scriptPubKey
        // At 4 WUs per byte
        result += txout_size * 4
    }

    // So that's it for a non-segwit tx
    // Now if this tx has witness data:
    if let Some(witness_data) = &tx.witness_data {
        // Flag + marker are 2 bytes so thats 2 WUs
        result += 2;

        for witness in witness_data.iter() {
            result += witness.len();
        }
    }

    result as u32
}

fn calculate_tx_size_wus_borrowed(tx: &TxBorrowed) -> u32 {
    let mut result = 0;
    // First we count the non-segwit data at 4 WUs per bytes
    // Version is 4 bytes so 16 WUs
    result += 16;
    // Locktime is also 4 bytes so another 16 WUs
    result += 16;

    // Depending on how many txins there are, the varint will take a diff amount of space: 4 * bytes
    result += 4 * length_varint(tx.txins.len() as VarInt);
    // Now for every txin:
    for txin in tx.txins.iter() {
        let txin_size = 32 + // length of previous tx hash: 32 bytes
			4 + // length of previous tx output index: 4 bytes
			4 + // length of sequence: 4 bytes
			length_varint(txin.sig_script.len() as VarInt) + // length of varint length of sigScript
			txin.sig_script.len(); // length of sigscript
        // at 4 WUs per byte:
        result += txin_size * 4
    }
    // Depending on how many txouts there are, the varint will take a diff amount of space: 4 * bytes
    result += 4 * length_varint(tx.txouts.len() as VarInt);
    // Now for every txout:
    for txout in tx.txouts.iter() {
        let txout_size = 8 + // size of value
			length_varint(txout.script.len() as VarInt) + // length of varint length of scriptPubKey
			txout.script.len(); // length of scriptPubKey
        // At 4 WUs per byte
        result += txout_size * 4
    }

    // So that's it for a non-segwit tx
    // Now if this tx has witness data:
    if let Some(witness_data) = &tx.witness_data {
        // Flag + marker are 2 bytes so thats 2 WUs
        result += 2;

        for witness in witness_data.iter() {
            result += witness.len();
        }
    }

    result as u32
}
