use crate::{api::AddressParam, chainman, db, types::addresstransaction::AddressTransaction};
use axum::{Json, extract::Query, http::StatusCode};
use bech32::{Fe32, hrp};

pub async fn address_data(
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
