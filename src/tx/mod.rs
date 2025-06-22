use crate::{
    db::{self, rocksdb::SerializedTx},
    packets::{
        tx::{Tx, TxOut},
        varint::{VarInt, varint_len, varint_serialize},
    },
    util::arena::Arena,
};

pub fn analyze_tx<'arena, 'data>(
    tx: &'data Tx,
    dependencies: &'data [&'data TxOut],
    arena: &'arena Arena,
) -> db::rocksdb::SerializedTx<'arena> {
    // TODO: actually analyze the tx, for now we just serialize txouts and return
    let analyzed_tx = arena
        .try_alloc_array_fill_copy(0, 0)
        .expect("to allocate analyzed_tx");
    let tx_outs = serialize_txouts(tx, arena);
    SerializedTx {
        hash: tx.hash,
        analyzed_tx: Some(analyzed_tx),
        tx_outs: Some(tx_outs),
        fee: 0,
        value: 0,
        size: 0,
    }
}

pub fn serialize_txouts<'arena>(tx: &Tx, arena: &'arena Arena) -> &'arena [u8] {
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
