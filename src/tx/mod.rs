use crate::{
    db,
    packets::tx::{Tx, TxOut},
};

pub fn analyze_tx(tx: &Tx, dependencies: &[&TxOut]) -> db::rocksdb::SerializedTx {
    todo!()
}
