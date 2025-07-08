use rocksdb::{SerializedTx, setup_rocksdb, write_block_txs, write_txouts, write_txs};
use sqlite::{mark_blocks_as_downloaded, setup_sqlite};

use crate::{
    packets::{
        packetpayload::SerializableValue,
        varint::{VarInt, varint_len},
    },
    types::blockmetrics::BlockMetrics,
};

mod batch;
mod batched;
mod migrations;
pub mod rocksdb;
pub mod sqlite;

pub async fn setup() {
    // sqlite for peers + headers
    setup_sqlite().await;

    // rocksdb for transactions
    setup_rocksdb().await;
}

pub async fn write_analyzed_txs(
    blocks: &[[u8; 32]],
    txs: &[&[SerializedTx<'_>]],
    block_metrics: &[BlockMetrics],
) {
    debug_assert_eq!(blocks.len(), txs.len());

    let serialized_txhashes: Vec<Vec<u8>> = txs
        .iter()
        .map(|block| block.iter().map(|tx| tx.hash).collect::<Vec<[u8; 32]>>())
        .map(|hashes| {
            let length = varint_len(hashes.len() as VarInt);
            let mut serialized = Vec::with_capacity(length + (hashes.len() * 32));
            (hashes.len() as VarInt).serialize(&mut serialized);

            for tx in hashes {
                serialized.extend_from_slice(&tx);
            }

            serialized
        })
        .collect();
    let mut blocks_with_txs_pairs: Vec<(&[u8; 32], Vec<u8>)> =
        blocks.iter().zip(serialized_txhashes.into_iter()).collect();
    // sort by hash before ingestion
    blocks_with_txs_pairs.sort_by(|a, b| a.0.cmp(b.0));

    let mut collapsed: Vec<&SerializedTx> = txs.into_iter().map(|x| x.iter()).flatten().collect();
    // sort by hash before ingestion
    collapsed.sort_by(|a, b| a.hash.cmp(&b.hash));

    async_scoped::TokioScope::scope_and_block(|s| {
        s.spawn(write_txouts(&collapsed));
        s.spawn(write_txs(&collapsed));
        s.spawn(write_block_txs(blocks_with_txs_pairs));
    });

    // after we're done with txs, update fetched_full in sqlite
    mark_blocks_as_downloaded(blocks, block_metrics);
}
