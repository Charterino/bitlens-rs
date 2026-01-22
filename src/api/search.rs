use axum::{Json, extract::Query, http::StatusCode};
use serde::{Deserialize, Serialize};
use slog_scope::warn;

use crate::{
    api::{SearchParam, address::parse_address},
    chainman, db,
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    address: Option<String>,
    #[serde(
        serialize_with = "crate::util::serialize_as_hex::serialize_option_hash_as_hex_reversed"
    )]
    block_hash: Option<[u8; 32]>,
    #[serde(
        serialize_with = "crate::util::serialize_as_hex::serialize_option_hash_as_hex_reversed"
    )]
    tx_hash: Option<[u8; 32]>,
}

pub async fn search(Query(params): Query<SearchParam>) -> Result<Json<SearchResponse>, StatusCode> {
    let nothing_found = SearchResponse {
        address: None,
        block_hash: None,
        tx_hash: None,
    };
    // the user might be searching for an:
    // - address
    // - block hash
    // - block number
    // - tx hash

    if let Some((address_bytes, human_address)) = parse_address(params.term.clone()) {
        // It's a valid address, just need to check if it has at least 1 tx and then we're gucci
        return match db::rocksdb::get_address_entires(address_bytes, 0, u64::MAX, 1).await {
            // Even if we have no records of this address, the get_address_entries call should still return Ok with an empty vec.
            Ok(v) => {
                if !v.is_empty() {
                    Ok(Json(SearchResponse {
                        address: Some(human_address.to_string()),
                        block_hash: None,
                        tx_hash: None,
                    }))
                } else {
                    Ok(Json(nothing_found))
                }
            }
            Err(e) => {
                warn!("failed to get address records for search"; "error" => e.to_string(), "for_address" => human_address.to_string());
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        };
    }

    if let Ok(block_number) = params.term.parse::<u64>() {
        return match chainman::get_hash_by_number(block_number) {
            Some(v) => Ok(Json(SearchResponse {
                address: None,
                block_hash: Some(v),
                tx_hash: None,
            })),
            None => Ok(Json(nothing_found)),
        };
    }

    // From here it's either a block hash or a tx hash
    let mut unhexxed = match hex::decode(params.term) {
        Err(_) => return Ok(Json(nothing_found)),
        Ok(v) => v,
    };
    if unhexxed.len() != 32 {
        return Ok(Json(nothing_found));
    }
    unhexxed.reverse();
    let unhexxed: [u8; 32] = unhexxed.try_into().unwrap();

    if chainman::get_header_by_hash(unhexxed).is_some() {
        return Ok(Json(SearchResponse {
            address: None,
            block_hash: Some(unhexxed),
            tx_hash: None,
        }));
    }

    if db::rocksdb::get_analyzed_tx(unhexxed).await.is_ok() {
        return Ok(Json(SearchResponse {
            address: None,
            block_hash: None,
            tx_hash: Some(unhexxed),
        }));
    }

    Ok(Json(nothing_found))
}
