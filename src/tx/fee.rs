use crate::packets::tx::{TxOutRef, TxRef};

pub fn calculate_fee(tx: TxRef, dependencies: &[TxOutRef]) -> u64 {
    let total_in: u64 = dependencies.iter().map(|x| x.value()).sum();
    let total_out: u64 = tx.txouts().map(|x| x.value()).sum();
    total_in - total_out
}
