use crate::{
    db::{self, rocksdb::SerializedTx},
    packets::{
        buffer::Buffer,
        tx::{TxOutRef, TxOwned, TxRef},
        varint::{VarInt, length_varint, serialize_varint_into_slice},
    },
    util::arena::Arena,
};
use fee::calculate_fee;
use flags::{SCRIPT_VERIFY_P2SH, SCRIPT_VERIFY_WITNESS};
use script::get_transaction_sigop_cost;
use serde::Serialize;
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzedTx {
    pub fee: u64,
    pub txouts_sum: u64,
    pub size_wus: u32,
    pub sigops: u32,
    pub tx: TxOwned,
}

pub fn deserialize_analyzed_tx(data: &[u8], hash: [u8; 32]) -> AnalyzedTx {
    let fee = data.get_u64_le(0).expect("to deserialize fee");
    let txouts_sum = data.get_u64_le(8).expect("to deserialize txouts_sum");
    let size_wus = data.get_u32_le(16).expect("to deserialize size_wus");
    let sigops = data.get_u32_le(20).expect("to deserialize sigops");

    let tx = TxOwned::deserialize_without_txouts(
        data.with_offset(24)
            .expect("to offset before deserializing tx"),
        hash,
    )
    .expect("to deserialize tx without txouts");

    AnalyzedTx {
        fee,
        txouts_sum,
        size_wus,
        sigops,
        tx,
    }
}

pub fn analyze_tx<'arena, 'data>(
    tx: TxRef<'data>,
    dependencies: &'data [TxOutRef<'data>],
    arena: &'arena Arena,
) -> db::rocksdb::SerializedTx<'arena> {
    let fee = if tx.is_coinbase() {
        0
    } else {
        calculate_fee(tx, dependencies)
    };

    let txouts_sum: u64 = tx.txouts().map(|x| x.value()).sum();

    let size_wus = calculate_tx_size_wus(tx);

    let mut flags = SCRIPT_VERIFY_P2SH;
    if tx.witness_data().is_some() {
        flags |= SCRIPT_VERIFY_WITNESS;
    }

    let sigops = get_transaction_sigop_cost(tx, dependencies, flags);

    let analyzed_tx = arena
        // serialized tx size + fee (8 bytes) + txouts_sum (8 bytes) + size_wus(4 bytes) + sigops (4 bytes)
        .try_alloc_array_fill_copy(tx.serialized_without_txouts_size() + 8 + 8 + 4 + 4, 0u8)
        .expect("to allocate space for analyzed tx");

    analyzed_tx[0..8].copy_from_slice(&fee.to_le_bytes());
    analyzed_tx[8..16].copy_from_slice(&txouts_sum.to_le_bytes());
    analyzed_tx[16..20].copy_from_slice(&size_wus.to_le_bytes());
    analyzed_tx[20..24].copy_from_slice(&sigops.to_le_bytes());
    let mut for_tx = &mut analyzed_tx[24..];
    tx.serialize_without_txouts(&mut for_tx);

    let tx_outs = serialize_txouts(tx, arena);
    SerializedTx {
        hash: tx.hash(),
        analyzed_tx: Some(analyzed_tx),
        tx_outs: Some(tx_outs),
        fee,
        txouts_sum,
        size_wus,
    }
}

fn serialize_txouts<'arena>(tx: TxRef, arena: &'arena Arena) -> &'arena [u8] {
    let mut size = length_varint(tx.txouts().count() as VarInt);
    for txout in tx.txouts() {
        size += txout.serialized_length();
    }
    let result = arena
        .try_alloc_array_fill_copy(size, 0u8)
        .expect("to allocate space for txouts");

    let mut offset = serialize_varint_into_slice(tx.txouts().count() as VarInt, result);
    for txout in tx.txouts() {
        let with_offset = result.get_mut(offset..).unwrap();
        offset += txout.flat_serialize(with_offset);
    }

    debug_assert_eq!(size, offset);

    result
}
