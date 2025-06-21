use crate::{
    db::{self, rocksdb::SerializedTx},
    packets::tx::{Tx, TxOut},
    util::arena::Arena,
};

pub fn analyze_tx<'arena, 'data>(
    tx: &'data Tx,
    dependencies: &'data [&'data TxOut],
    arena: &'arena Arena,
) -> db::rocksdb::SerializedTx<'arena> {
    let analyzed_tx = arena
        .try_alloc_array_fill_copy(0, 0)
        .expect("to have allocated analyzed_tx");
    let tx_outs = arena
        .try_alloc_array_fill_copy(0, 0)
        .expect("to have allocated tx_outs");
    SerializedTx {
        hash: tx.hash,
        analyzed_tx: Some(analyzed_tx),
        tx_outs: Some(tx_outs),
        fee: 0,
        value: 0,
        size: 0,
    }
}
