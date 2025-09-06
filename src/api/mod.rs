use std::ops::Add;

use crate::{
    chainman::{self, FRONTPAGE_STATS},
    db::{self, rocksdb::BlockTxEntry},
    packets::tx::{TxInOwned, TxOutOwned, TxRef},
    tx::{AnalyzedTx, address::Address, get_human_address_from_script},
    types::{addresstransaction::AddressTransaction, blockheaderwithnumber::BlockHeaderWithNumber},
};
use axum::{
    Json, Router,
    extract::Query,
    http::{HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::get,
};
use bech32::{Fe32, hrp};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

pub async fn start() {
    let app = Router::new()
        .route("/api/frontpagedata", get(frontpagedata))
        .route("/api/tx", get(txdata))
        .route("/api/block", get(blockdata))
        .route("/api/address", get(address_data))
        .layer(
            CorsLayer::new()
                .allow_origin("*".parse::<HeaderValue>().unwrap())
                .allow_methods([Method::GET]),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8122").await.unwrap();
    tokio::spawn(async { axum::serve(listener, app).await });
}

async fn frontpagedata() -> impl IntoResponse {
    let r = FRONTPAGE_STATS.read().unwrap();
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        r.serialized.clone(),
    )
}

#[derive(Debug, Deserialize)]
struct HashParam {
    hash: String,
}

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

async fn txdata(Query(params): Query<HashParam>) -> Result<Json<TxDataResponse>, StatusCode> {
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
    let mut handles = Vec::with_capacity(txins.len());
    for txin in txins {
        let hash = txin.prevout_hash;
        let index = txin.prevout_index;
        handles.push(tokio::spawn(async move {
            let mut txouts = db::rocksdb::get_transaction_outputs(hash)
                .await
                .expect("to have found txouts in the db");

            txouts.swap_remove(index as usize)
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        results.push(handle.await?);
    }

    Ok(results)
}

fn txouts_to_addresses(txouts: &[TxOutOwned]) -> Vec<Address> {
    if txouts.len() == 0 {
        return vec![Address::Coinbase];
    }
    txouts
        .iter()
        .map(|txout| get_human_address_from_script(&txout.script))
        .collect()
}

#[derive(Serialize, Deserialize)]
pub struct BlockDataResponse {
    #[serde(flatten)]
    pub header: BlockHeaderWithNumber,
    pub txs: Vec<BlockTxEntry>,
}

async fn blockdata(Query(params): Query<HashParam>) -> Result<Json<BlockDataResponse>, StatusCode> {
    let mut unhexxed = match hex::decode(params.hash) {
        Err(_) => return Err(StatusCode::BAD_REQUEST),
        Ok(v) => v,
    };
    if unhexxed.len() != 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    unhexxed.reverse();
    let unhexxed: [u8; 32] = unhexxed.try_into().unwrap();
    let header = match chainman::get_header_by_hash(unhexxed) {
        None => return Err(StatusCode::NOT_FOUND),
        Some(v) => v,
    };

    let txs = db::rocksdb::get_block_tx_entries(unhexxed)
        .await
        .unwrap_or_default();

    Ok(Json(BlockDataResponse { header, txs }))
}

#[derive(Debug, Deserialize)]
struct AddressParam {
    address: String,
}

async fn address_data(
    Query(params): Query<AddressParam>,
) -> Result<Json<Vec<AddressTransaction>>, StatusCode> {
    let address_bytes = match parse_address(&params.address) {
        None => return Err(StatusCode::BAD_REQUEST),
        Some(b) => b,
    };

    let amends = db::rocksdb::get_address_entires(address_bytes)
        .await
        .unwrap_or_default();

    let mut transactions = chainman::filter_and_populate_address_txs(amends);
    transactions.sort_unstable_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(Json(transactions))
}

fn parse_address(address: &str) -> Option<Vec<u8>> {
    if let Ok(b) = hex::decode(address) {
        if b.len() == 65 || b.len() == 33 {
            // p2pk
            return Some(b);
        }
        return None;
    }
    if let Ok(mut b) = bs58::decode(address).with_check(Some(0x00)).into_vec() {
        if b.len() == 21 && b[0] == 0x00 {
            // p2pkh
            b.remove(0);
            return Some(b);
        }
        return None;
    }
    if let Ok(mut b) = bs58::decode(address).with_check(Some(0x05)).into_vec() {
        if b.len() == 21 && b[0] == 0x05 {
            // p2sh
            b.remove(0);
            return Some(b);
        }
        return None;
    }
    if let Ok((hrp, fe, b)) = bech32::segwit::decode(address) {
        if hrp != hrp::BC {
            return None;
        }
        if fe == Fe32::Q && (b.len() == 20 || b.len() == 32) {
            // p2wsh or p2wph
            return Some(b);
        }
        if fe == Fe32::P && b.len() == 32 {
            // taproot
            return Some(b);
        }
    }

    None
}
