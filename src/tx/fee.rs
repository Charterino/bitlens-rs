use crate::packets::tx::{Tx, TxOut};

pub fn calculate_fee(tx: &Tx, dependencies: &[&TxOut]) -> u64 {
    let total_in: u64 = dependencies.iter().map(|x| x.value).sum();
    let total_out: u64 = tx.txouts.inner.iter().map(|x| x.value).sum();
    total_in - total_out
}
