use crate::{
    api::{AddressContinueParam, AddressExtraParam, AddressTopParam},
    chainman, db,
    packets::tx::TxRef,
    tx::{AnalyzedTx, address::Address, get_human_address_from_script},
    types::addresstransaction::AddressTransaction,
};
use anyhow::anyhow;
use axum::{Json, extract::Query, http::StatusCode};
use bech32::{Fe32, hrp};
use serde::Serialize;
use slog_scope::warn;
use tokio::task::JoinHandle;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressTopResponse {
    pub total_txs: usize,
    pub total_spent: u64,
    pub total_received: u64,
    pub first_seen: u64,
    pub top_txs: Vec<AddressTransaction>,
}

pub async fn address_data_top(
    Query(params): Query<AddressTopParam>,
) -> Result<Json<AddressTopResponse>, StatusCode> {
    let (address_bytes, human_address) = match parse_address(params.address) {
        None => return Err(StatusCode::BAD_REQUEST),
        Some(b) => b,
    };

    let limit = params.limit.unwrap_or(50);

    let top_data = match db::rocksdb::get_top_address_data(address_bytes.clone(), limit).await {
        Ok(v) => v,
        Err(e) => {
            warn!("failed to get top address data"; "error" => e.to_string(), "for_address" => human_address.to_string());
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let mut transactions = chainman::filter_and_populate_address_txs(top_data.transactions);
    transactions.sort_unstable_by(|a, b| b.timestamp.cmp(&a.timestamp));

    populate_other_address_fields(human_address, &mut transactions).await?;

    Ok(Json(AddressTopResponse {
        total_txs: top_data.total_txs,
        top_txs: transactions,
        total_spent: top_data.total_spent,
        total_received: top_data.total_received,
        first_seen: top_data.first_seen,
    }))
}

pub async fn address_data_extra(
    Query(params): Query<AddressExtraParam>,
) -> Result<Json<Vec<AddressTransaction>>, StatusCode> {
    let (address_bytes, human_address) = match parse_address(params.address) {
        None => return Err(StatusCode::BAD_REQUEST),
        Some(b) => b,
    };

    let from_timestamp = params.from_timestamp.unwrap_or(0);
    let to_timestamp = params.to_timestamp.unwrap_or(u64::MAX);
    let limit = params.limit.unwrap_or(50);

    let amends =
        db::rocksdb::get_address_entires(address_bytes, from_timestamp, to_timestamp, limit)
            .await
            .unwrap_or_default();

    let mut transactions = chainman::filter_and_populate_address_txs(amends);
    transactions.sort_unstable_by(|a, b| b.timestamp.cmp(&a.timestamp));

    populate_other_address_fields(human_address, &mut transactions).await?;

    Ok(Json(transactions))
}

pub async fn address_data_continue(
    Query(params): Query<AddressContinueParam>,
) -> Result<Json<Vec<AddressTransaction>>, StatusCode> {
    let (address_bytes, human_address) = match parse_address(params.address) {
        None => return Err(StatusCode::BAD_REQUEST),
        Some(b) => b,
    };

    let mut unhexxed = match hex::decode(params.after_tx) {
        Err(_) => return Err(StatusCode::BAD_REQUEST),
        Ok(v) => v,
    };
    if unhexxed.len() != 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    unhexxed.reverse();
    let unhexxed: [u8; 32] = unhexxed.try_into().unwrap();
    let limit = params.limit.unwrap_or(50);

    let amends = db::rocksdb::get_address_entires_continue(
        address_bytes,
        unhexxed,
        params.after_timestamp,
        limit,
    )
    .await
    .unwrap_or_default();

    let mut transactions = chainman::filter_and_populate_address_txs(amends);
    transactions.sort_unstable_by(|a, b| b.timestamp.cmp(&a.timestamp));

    populate_other_address_fields(human_address, &mut transactions).await?;

    Ok(Json(transactions))
}

async fn populate_other_address_fields(
    human_address: Address,
    transactions: &mut [AddressTransaction],
) -> Result<(), StatusCode> {
    // The following ~90 lines or so are pretty ugly so here's an explanation:
    // First we fetch full txs for every relevant transaction for this address
    // Then for every full transaction in which the address gains sats, we have to pull the txouts from the store to see where the sats come from.
    // Then for every full transaction in which the address loses sats, we have to pull the txouts from the store to see where the sats went.

    let full_txs_tasks = transactions
        .iter()
        .map(|x| tokio::spawn(db::rocksdb::get_analyzed_tx(x.transaction_hash)))
        .collect::<Vec<JoinHandle<anyhow::Result<AnalyzedTx>>>>();

    let mut full_txs_results = Vec::with_capacity(full_txs_tasks.len());
    for task in full_txs_tasks {
        full_txs_results.push(task.await.unwrap_or(Err(anyhow!("cancelled"))));
    }
    let full_txs = match full_txs_results
        .into_iter()
        .collect::<anyhow::Result<Vec<AnalyzedTx>>>()
    {
        Ok(v) => v,
        Err(e) => {
            warn!("failed to fetch analyzed txs for address_data_top"; "error" => e.to_string(), "for_address" => human_address.to_string());
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let mut gain_txouts_tasks = Vec::with_capacity(transactions.len());
    for (_, analyzed_tx) in
        transactions
            .iter()
            .zip(full_txs.iter())
            .filter(|(address_tx, analyzed_tx)| {
                address_tx.value > 0 && !TxRef::Owned(&analyzed_tx.tx).is_coinbase()
            })
    {
        let cloned_txins = analyzed_tx.tx.txins.clone();
        gain_txouts_tasks.push(tokio::spawn(async move {
            let mut result = Vec::with_capacity(cloned_txins.len());
            for txin in &cloned_txins {
                let txouts = match db::rocksdb::get_transaction_outputs(txin.prevout_hash).await {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };

                result.push(txouts[txin.prevout_index as usize].clone())
            }
            Ok(result)
        }));
    }
    let mut gain_txouts = Vec::with_capacity(gain_txouts_tasks.len());
    for task in gain_txouts_tasks {
        match task.await {
            Ok(Ok(v)) => {
                gain_txouts.push(v);
            }
            Ok(Err(e)) => {
                // TODO: better logging here ig
                warn!("failed to fetch txouts for address_data_top"; "error" => e.to_string(), "for_address" => human_address.to_string());
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            Err(_) => {
                // cancelled
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    let mut lose_txouts_tasks = Vec::with_capacity(transactions.len());
    for (_, analyzed_tx) in
        transactions
            .iter()
            .zip(full_txs.iter())
            .filter(|(address_tx, analyzed_tx)| {
                address_tx.value < 0 && !TxRef::Owned(&analyzed_tx.tx).is_coinbase()
            })
    {
        let hash = analyzed_tx.tx.hash;
        lose_txouts_tasks.push(tokio::spawn(async move {
            db::rocksdb::get_transaction_outputs(hash).await
        }));
    }
    let mut lose_txouts = Vec::with_capacity(lose_txouts_tasks.len());
    for task in lose_txouts_tasks {
        match task.await {
            Ok(Ok(v)) => {
                lose_txouts.push(v);
            }
            Ok(Err(e)) => {
                // TODO: better logging here ig
                warn!("failed to fetch txouts for address_data_top"; "error" => e.to_string(), "for_address" => human_address.to_string());
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            Err(_) => {
                // cancelled
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    let mut gain_txouts_index = 0usize;
    let mut lose_txouts_index = 0usize;
    for (address_tx, analyzed_tx) in transactions.iter_mut().zip(full_txs) {
        if address_tx.value > 0 {
            // they gained sats so check who it come from
            if TxRef::Owned(&analyzed_tx.tx).is_coinbase() {
                address_tx.distinct_other_addresses = 1;
                address_tx.single_other_address = Some(Address::Coinbase)
            } else {
                let relevant_txouts = &gain_txouts[gain_txouts_index];
                gain_txouts_index += 1;

                let mut other_addresses = relevant_txouts
                    .iter()
                    .map(|x| get_human_address_from_script(&x.script))
                    .filter(|address| *address != human_address)
                    .collect::<Vec<Address>>();
                other_addresses.sort();
                other_addresses.dedup();

                address_tx.distinct_other_addresses = other_addresses.len();
                if other_addresses.len() == 1 {
                    address_tx.single_other_address = Some(other_addresses[0].clone())
                }
            }
        } else if address_tx.value < 0 {
            // they lost sats so check where it go

            let txouts = &lose_txouts[lose_txouts_index];
            lose_txouts_index += 1;
            let mut other_addresses = txouts
                .iter()
                .map(|x| get_human_address_from_script(&x.script))
                .filter(|address| *address != human_address)
                .collect::<Vec<Address>>();
            other_addresses.sort();
            other_addresses.dedup();

            address_tx.distinct_other_addresses = other_addresses.len();
            if other_addresses.len() == 1 {
                address_tx.single_other_address = Some(other_addresses[0].clone())
            }
        }
    }

    Ok(())
}

pub fn parse_address(address: String) -> Option<(Vec<u8>, Address)> {
    if let Ok(b) = hex::decode(&address) {
        if b.len() == 65 {
            // p2pk full
            return Some((b, Address::P2PKFull { value: address }));
        } else if b.len() == 33 {
            // p2pk short
            return Some((b, Address::P2PKShort { value: address }));
        }
        return None;
    }
    if let Ok(mut b) = bs58::decode(&address).with_check(Some(0x00)).into_vec() {
        if b.len() == 21 && b[0] == 0x00 {
            // p2pkh
            b.remove(0);
            return Some((b, Address::P2PKH { value: address }));
        }
        return None;
    }
    if let Ok(mut b) = bs58::decode(&address).with_check(Some(0x05)).into_vec() {
        if b.len() == 21 && b[0] == 0x05 {
            // p2sh
            b.remove(0);
            return Some((b, Address::P2SH { value: address }));
        }
        return None;
    }
    if let Ok((hrp, fe, b)) = bech32::segwit::decode(&address) {
        if hrp != hrp::BC {
            return None;
        }
        if fe == Fe32::Q {
            if b.len() == 20 {
                // p2wsh
                return Some((b, Address::P2WSH { value: address }));
            } else if b.len() == 32 {
                // p2wph
                return Some((b, Address::P2WPKH { value: address }));
            }
        } else if fe == Fe32::P && b.len() == 32 {
            // taproot
            return Some((b, Address::Taproot { value: address }));
        }
    }

    None
}
