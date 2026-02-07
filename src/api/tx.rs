use crate::{
    api::HashParam,
    chainman, db,
    packets::tx::{TxInOwned, TxOutOwned, TxRef},
    tx::{AnalyzedTx, address::Address, get_human_address_from_script},
    types::blockheaderwithnumber::BlockHeaderWithNumber,
};
use anyhow::Context;
use axum::{Json, extract::Query, http::StatusCode};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

#[derive(Serialize, Deserialize)]
pub struct TxDataResponse {
    pub header: BlockHeaderWithNumber,
    #[serde(flatten)]
    pub tx: AnalyzedTx,
    #[serde(serialize_with = "crate::util::serialize_spends::serialize_spends")]
    pub spends: Vec<(u32, [u8; 32])>,
    pub txin_addresses: Vec<Address>,
    pub txout_addresses: Vec<Address>,
}

pub async fn txdata(Query(params): Query<HashParam>) -> Result<Json<TxDataResponse>, StatusCode> {
    let mut unhexxed = match hex::decode(params.hash) {
        Err(_) => return Err(StatusCode::BAD_REQUEST),
        Ok(v) => v,
    };
    if unhexxed.len() != 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    unhexxed.reverse();
    let unhexxed: [u8; 32] = unhexxed.try_into().unwrap();
    let mut tx = match db::rocksdb::get_analyzed_tx(unhexxed).await {
        Err(_) => return Err(StatusCode::NOT_FOUND),
        Ok(v) => v,
    };

    let txouts = match db::rocksdb::get_transaction_outputs(unhexxed).await {
        Err(_) => return Err(StatusCode::NOT_FOUND),
        Ok(v) => v,
    };

    tx.tx.txouts = txouts;
    tx.tx.compute_witness_hash();

    let header = match chainman::get_header_by_hash(tx.block_hash) {
        Some(h) => h,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let spends = match db::rocksdb::get_tx_spends(unhexxed).await {
        Ok(v) => v,
        Err(_) => return Err(StatusCode::NOT_FOUND),
    };

    let filtered_spends = chainman::filter_tx_spends(spends);

    let txout_addresses = txouts_to_addresses(&tx.tx.txouts);

    let txouts_for_txins = if TxRef::Owned(&tx.tx).is_coinbase() {
        vec![]
    } else {
        match get_txouts_for_txins(&tx.tx.txins).await {
            Ok(v) => v,
            Err(_) => return Err(StatusCode::NOT_FOUND),
        }
    };

    Ok(Json(TxDataResponse {
        header,
        tx,
        spends: filtered_spends,
        txout_addresses,
        txin_addresses: txouts_to_addresses(&txouts_for_txins),
    }))
}

async fn get_txouts_for_txins(txins: &[TxInOwned]) -> anyhow::Result<Vec<TxOutOwned>> {
    let mut handles: Vec<JoinHandle<anyhow::Result<TxOutOwned>>> = Vec::with_capacity(txins.len());
    for txin in txins {
        let hash = txin.prevout_hash;
        let index = txin.prevout_index;
        handles.push(tokio::spawn(async move {
            let mut txouts = db::rocksdb::get_transaction_outputs(hash).await?;
            Ok(txouts.swap_remove(index as usize))
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        results.push(handle.await??);
    }

    Ok(results)
}

fn txouts_to_addresses(txouts: &[TxOutOwned]) -> Vec<Address> {
    if txouts.is_empty() {
        return vec![Address::Coinbase];
    }
    txouts
        .iter()
        .map(|txout| get_human_address_from_script(&txout.script))
        .collect()
}
