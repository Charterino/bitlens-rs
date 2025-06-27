use crate::{
    db::{self, rocksdb::SerializedTx},
    packets::{
        tx::{Tx, TxOut},
        varint::{VarInt, varint_len, varint_serialize},
    },
    util::arena::Arena,
};
use fee::calculate_fee;
use flags::{SCRIPT_VERIFY_P2SH, SCRIPT_VERIFY_WITNESS};
use script::get_transaction_sigop_cost;
use size::calculate_tx_size_wus;

mod fee;
mod flags;
mod opcodes;
mod script;
mod size;

#[cfg(test)]
mod script_test;
#[cfg(test)]
mod size_test;

pub fn analyze_tx<'arena, 'data>(
    tx: &'data Tx,
    dependencies: &'data [&'data TxOut],
    arena: &'arena Arena,
) -> db::rocksdb::SerializedTx<'arena> {
    let fee = if tx.is_coinbase() {
        0
    } else {
        calculate_fee(tx, dependencies)
    };

    let size_wus = calculate_tx_size_wus(tx);

    let mut flags = SCRIPT_VERIFY_P2SH;
    if tx.witness_data.is_some() {
        flags |= SCRIPT_VERIFY_WITNESS;
    }

    let sigops = get_transaction_sigop_cost(tx, dependencies, flags);

    let tx_outs = serialize_txouts(tx, arena);
    SerializedTx {
        hash: tx.hash,
        analyzed_tx: Some(todo!()),
        tx_outs: Some(tx_outs),
        fee: 0,
        value: 0,
        size: 0,
    }
}

fn serialize_txouts<'arena>(tx: &Tx, arena: &'arena Arena) -> &'arena [u8] {
    let len_size = varint_len(tx.txouts.inner.len() as VarInt);
    let mut size = len_size;
    for txout in tx.txouts.inner.iter() {
        size += txout.serialized_length();
    }
    let result = arena
        .try_alloc_array_fill_copy(size, 0u8)
        .expect("to allocate space for txouts");

    varint_serialize(tx.txouts.inner.len() as VarInt, result);
    let mut new_size = len_size;
    for txout in tx.txouts.inner.iter() {
        let with_offset = result.get_mut(new_size..).unwrap();
        new_size += txout.flat_serialize(with_offset);
    }

    debug_assert_eq!(size, new_size);

    result
}
