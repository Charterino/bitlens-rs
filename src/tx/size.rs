use crate::packets::{
    tx::Tx,
    varint::{VarInt, varint_len},
};

pub fn calculate_tx_size_wus(tx: &Tx) -> u64 {
    let mut result = 0;
    // First we count the non-segwit data at 4 WUs per bytes
    // Version is 4 bytes so 16 WUs
    result += 16;
    // Locktime is also 4 bytes so another 16 WUs
    result += 16;

    // Depending on how many txins there are, the varint will take a diff amount of space: 4 * bytes
    result += 4 * varint_len(tx.txins.inner.len() as VarInt);
    // Now for every txin:
    for txin in tx.txins.inner.iter() {
        let txin_size = 32 + // length of previous tx hash: 32 bytes
			4 + // length of previous tx output index: 4 bytes
			4 + // length of sequence: 4 bytes
			varint_len(txin.sig_script.inner.len() as VarInt) + // length of varint length of sigScript
			txin.sig_script.inner.len(); // length of sigscript
        // at 4 WUs per byte:
        result += txin_size * 4
    }
    // Depending on how many txouts there are, the varint will take a diff amount of space: 4 * bytes
    result += 4 * varint_len(tx.txouts.inner.len() as VarInt);
    // Now for every txout:
    for txout in tx.txouts.inner.iter() {
        let txout_size = 8 + // size of value
			varint_len(txout.script.inner.len() as VarInt) + // length of varint length of scriptPubKey
			txout.script.inner.len(); // length of scriptPubKey
        // At 4 WUs per byte
        result += txout_size * 4
    }

    // So that's it for a non-segwit tx
    // Now if this tx has witness data:
    if let Some(witness_data) = &tx.witness_data {
        // Flag + marker are 2 bytes so thats 2 WUs
        result += 2;

        for witness in witness_data.inner.iter() {
            // length of the number of components in this witness block
            result += varint_len(witness.inner.len() as VarInt);
            for component in witness.inner.iter() {
                // Length of the length of this component
                result += varint_len(component.inner.len() as VarInt);
                // Length of this component
                result += component.inner.len();
            }
        }
    }

    result as u64
}
