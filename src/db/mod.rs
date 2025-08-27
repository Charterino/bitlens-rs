use crate::{
    db::rocksdb::{BlockTxEntry, write_txspends},
    packets::varint::{VarInt, length_varint, serialize_varint},
    types::blockmetrics::BlockMetrics,
};
use rocksdb::{SerializedTx, setup_rocksdb, write_block_txs, write_txouts, write_txs};
use sqlite::{mark_blocks_as_downloaded, setup_sqlite};

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
        .map(|block_txs| {
            let length = length_varint(block_txs.len() as VarInt);
            let mut serialized =
                Vec::with_capacity(length + (block_txs.len() * size_of::<BlockTxEntry>()));
            let initial_cap = serialized.capacity();
            serialize_varint(block_txs.len() as VarInt, &mut serialized);

            for tx in block_txs.iter() {
                bincode::encode_into_std_write(
                    BlockTxEntry {
                        hash: tx.hash,
                        value: tx.txouts_sum as f64 / 100_000_000.,
                        fee_sats: tx.fee,
                        size_wus: tx.size_wus,
                    },
                    &mut serialized,
                    bincode::config::standard(),
                )
                .expect("to serialize blocktxentry");
            }

            assert_eq!(initial_cap, serialized.capacity());

            serialized
        })
        .collect();
    let mut blocks_with_txs_pairs: Vec<(&[u8; 32], Vec<u8>)> =
        blocks.iter().zip(serialized_txhashes.into_iter()).collect();
    // sort by hash before ingestion
    blocks_with_txs_pairs.sort_by(|a, b| a.0.cmp(b.0));

    let mut collapsed: Vec<&SerializedTx> = txs.iter().flat_map(|x| x.iter()).collect();
    // sort by hash before ingestion
    collapsed.sort_by(|a, b| a.hash.cmp(&b.hash));

    let mut spends = {
        let mut result = Vec::with_capacity(collapsed.len());
        for i in 0..blocks.len() {
            let block_hash = blocks[i];
            for tx in txs[i] {
                if tx.spent_txos.is_none() {
                    continue;
                }
                let mut value = [0u8; 64];
                value[0..32].copy_from_slice(&tx.hash);
                value[32..64].copy_from_slice(&block_hash);
                for txin in tx.spent_txos.unwrap() {
                    result.push((*txin, value));
                }
            }
        }
        result
    };
    // sort by hash before ingestion
    spends.sort_by(|a, b| a.0.cmp(b.0));

    async_scoped::TokioScope::scope_and_block(|s| {
        s.spawn(write_txouts(&collapsed));
        s.spawn(write_txs(&collapsed));
        s.spawn(write_block_txs(blocks_with_txs_pairs));
        if !spends.is_empty() {
            s.spawn(write_txspends(spends));
        }
    });

    // after we're done with txs, update fetched_full in sqlite
    mark_blocks_as_downloaded(blocks, block_metrics);
}
